use crate::data::error_info::ErrorInfo;
use crate::data::literal::ContentType;
use crate::data::primitive::{closure::capture_variables, PrimitiveArray, PrimitiveObject};
use crate::data::{
    ast::*, warnings::DisplayWarnings, ArgsType, Data, Literal, MemoryType, MessageData, Position,
    MSG,
};
use crate::error_format::*;
use crate::interpreter::{
    ast_interpreter::evaluate_condition,
    variable_handler::{
        exec_path_actions, get_string_from_complex_string, get_var, interval::interval_from_expr,
        operations::evaluate_postfix, resolve_csml_object::resolve_object, resolve_path,
    },
};
use std::{collections::HashMap, sync::mpsc};

////////////////////////////////////////////////////////////////////////////////
// PRIVATE FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

async fn exec_path_literal(
    literal: &mut Literal,
    dis_warnings: &DisplayWarnings,
    path: Option<&[(Interval, PathState)]>,
    data: &mut Data<'_>,
    msg_data: &mut MessageData,
    sender: &Option<mpsc::Sender<MSG>>,
) -> Result<Literal, ErrorInfo> {
    if let Some(path) = path {
        let path = Box::pin(resolve_path(path, dis_warnings, data, msg_data, sender)).await?;
        let (new_literal, ..) = exec_path_actions(
            literal,
            dis_warnings,
            &MemoryType::Use,
            None,
            &Some(path),
            &ContentType::get(literal),
            data,
            msg_data,
            sender,
        )
        .await?;

        Ok(new_literal)
    } else {
        Ok(literal.to_owned())
    }
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTION
////////////////////////////////////////////////////////////////////////////////

pub async fn expr_to_literal(
    expr: &Expr,
    dis_warnings: &DisplayWarnings,
    path: Option<&[(Interval, PathState)]>,
    data: &mut Data<'_>,
    msg_data: &mut MessageData,
    sender: &Option<mpsc::Sender<MSG>>,
) -> Result<Literal, ErrorInfo> {
    match expr {
        Expr::ObjectExpr(ObjectType::As(name, var)) => {
            let value = Box::pin(expr_to_literal(
                var,
                dis_warnings,
                None,
                data,
                msg_data,
                sender,
            ))
            .await?;
            data.step_vars.insert(name.ident.to_owned(), value.clone());
            Ok(value)
        }
        Expr::PathExpr { literal, path } => {
            Box::pin(expr_to_literal(
                literal,
                dis_warnings,
                Some(path),
                data,
                msg_data,
                sender,
            ))
            .await
        }
        Expr::ObjectExpr(ObjectType::BuiltIn(Function {
            name,
            args,
            interval,
        })) => {
            let mut literal = resolve_object(name, args, *interval, data, msg_data, sender).await?;

            exec_path_literal(&mut literal, dis_warnings, path, data, msg_data, sender).await
        }
        Expr::MapExpr {
            object,
            interval: range_interval,
            ..
        } => {
            let mut map = HashMap::new();
            let mut is_secure = false;

            for (key, value) in object.iter() {
                let lit = Box::pin(expr_to_literal(
                    value,
                    dis_warnings,
                    None,
                    data,
                    msg_data,
                    sender,
                ))
                .await?;
                if lit.secure_variable {
                    is_secure = true;
                }

                map.insert(key.to_owned(), lit);
            }

            let mut literal = PrimitiveObject::get_literal(&map, range_interval.to_owned());
            literal.secure_variable = is_secure;

            exec_path_literal(&mut literal, dis_warnings, path, data, msg_data, sender).await
        }
        Expr::ComplexLiteral(vec, range_interval) => {
            let mut string = Box::pin(get_string_from_complex_string(
                vec,
                range_interval.to_owned(),
                data,
                msg_data,
                sender,
            ))
            .await?;
            exec_path_literal(&mut string, dis_warnings, path, data, msg_data, sender).await
        }
        Expr::VecExpr(vec, range_interval) => {
            let mut array = vec![];
            let mut is_secure = false;

            for value in vec.iter() {
                let lit = Box::pin(expr_to_literal(
                    value,
                    dis_warnings,
                    None,
                    data,
                    msg_data,
                    sender,
                ))
                .await?;
                if lit.secure_variable {
                    is_secure = true;
                }

                array.push(lit)
            }
            let mut literal = PrimitiveArray::get_literal(&array, range_interval.to_owned());
            literal.secure_variable = is_secure;

            exec_path_literal(&mut literal, dis_warnings, path, data, msg_data, sender).await
        }
        Expr::PostfixExpr(pretfix, expr) => {
            let mut literal =
                Box::pin(evaluate_postfix(pretfix, expr, data, msg_data, sender)).await?;
            exec_path_literal(&mut literal, dis_warnings, path, data, msg_data, sender).await
        }
        Expr::InfixExpr(infix, exp_1, exp_2) => {
            let mut literal = Box::pin(evaluate_condition(
                infix, exp_1, exp_2, data, msg_data, sender,
            ))
            .await?;
            exec_path_literal(&mut literal, dis_warnings, path, data, msg_data, sender).await
        }
        Expr::LitExpr { literal, .. } => {
            let mut new_value = exec_path_literal(
                &mut literal.clone(),
                dis_warnings,
                path,
                data,
                msg_data,
                sender,
            )
            .await?;
            // only for closure capture the step variables
            let memory: HashMap<String, Literal> = data.get_all_memories();
            capture_variables(&mut new_value, memory, &data.context.flow);
            Ok(new_value)
        }
        Expr::IdentExpr(var, ..) => {
            Ok(get_var(var.to_owned(), dis_warnings, path, data, msg_data, sender).await?)
        }
        e => Err(gen_error_info(
            Position::new(interval_from_expr(e), &data.context.flow),
            ERROR_EXPR_TO_LITERAL.to_owned(),
        )),
    }
}

pub async fn resolve_fn_args(
    expr: &Expr,
    data: &mut Data<'_>,
    msg_data: &mut MessageData,
    dis_warnings: &DisplayWarnings,
    sender: &Option<mpsc::Sender<MSG>>,
) -> Result<ArgsType, ErrorInfo> {
    match expr {
        Expr::VecExpr(vec, ..) => {
            let mut map = HashMap::new();
            let mut first = 0;
            let mut named_args = false;

            for (index, value) in vec.iter().enumerate() {
                match value {
                    Expr::ObjectExpr(ObjectType::Assign(_assign_type, name, var)) => {
                        let name = match **name {
                            Expr::IdentExpr(ref var, ..) => var,
                            _ => {
                                return Err(gen_error_info(
                                    Position::new(interval_from_expr(name), &data.context.flow),
                                    "key must be of type string".to_owned(),
                                ))
                            }
                        };
                        named_args = true;

                        let literal = Box::pin(expr_to_literal(
                            var,
                            dis_warnings,
                            None,
                            data,
                            msg_data,
                            sender,
                        ))
                        .await?;
                        map.insert(name.ident.to_owned(), literal);
                    }
                    expr => {
                        first += 1;
                        if named_args && first > 1 {
                            return Err(gen_error_info(
                                Position::new(interval_from_expr(expr), &data.context.flow),
                                ERROR_EXPR_TO_LITERAL.to_owned(),
                            ));
                        }
                        let literal = Box::pin(expr_to_literal(
                            expr,
                            dis_warnings,
                            None,
                            data,
                            msg_data,
                            sender,
                        ))
                        .await?;
                        map.insert(format!("arg{}", index), literal);
                    }
                }
            }

            match named_args {
                true => Ok(ArgsType::Named(map)),
                false => Ok(ArgsType::Normal(map)),
            }
        }
        e => Err(gen_error_info(
            Position::new(interval_from_expr(e), &data.context.flow),
            ERROR_EXPR_TO_LITERAL.to_owned(),
        )),
    }
}
