use crate::data::{
    ast::{Block, Expr, IfStatement, Infix, InstructionInfo},
    context::ContextStepInfo,
    warnings::DisplayWarnings,
    Data, Literal, MessageData, MSG,
};
use crate::error_format::*;
use crate::interpreter::{
    interpret_scope,
    variable_handler::{
        expr_to_literal, get_var,
        operations::{evaluate_infix, evaluate_postfix, valid_literal},
    },
};
use std::sync::mpsc;

////////////////////////////////////////////////////////////////////////////////
// PRIVATE FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

//TODO: add warning when comparing some objects
pub async fn valid_condition(
    expr: &Expr,
    data: &mut Data<'_>,
    msg_data: &mut MessageData,
    sender: &Option<mpsc::Sender<MSG>>,
) -> bool {
    match expr {
        Expr::LitExpr { literal, .. } => valid_literal(Ok(literal.to_owned())),
        Expr::IdentExpr(ident) => valid_literal(
            get_var(
                ident.to_owned(),
                &DisplayWarnings::Off,
                None,
                data,
                msg_data,
                sender,
            )
            .await,
        ),
        Expr::PostfixExpr(post, exp) => {
            valid_literal(evaluate_postfix(post, exp, data, msg_data, sender).await)
        }
        Expr::InfixExpr(inf, exp_1, exp_2) => valid_literal(
            Box::pin(evaluate_condition(
                inf, exp_1, exp_2, data, msg_data, sender,
            ))
            .await,
        ),
        value => valid_literal(
            expr_to_literal(value, &DisplayWarnings::Off, None, data, msg_data, sender).await,
        ),
    }
}

async fn evaluate_if_condition(
    cond: &Expr,
    mut msg_data: MessageData,
    data: &mut Data<'_>,
    consequence: &Block,
    instruction_info: &InstructionInfo,
    sender: &Option<mpsc::Sender<MSG>>,
    then_branch: &Option<Box<IfStatement>>,
) -> Result<MessageData, ErrorInfo> {
    if valid_condition(cond, data, &mut msg_data, sender).await {
        msg_data = msg_data + Box::pin(interpret_scope(consequence, data, sender)).await?;
        return Ok(msg_data);
    }
    if let Some(then) = then_branch {
        Box::pin(solve_if_statement(
            then,
            msg_data,
            data,
            instruction_info,
            sender,
        ))
        .await
    } else {
        Ok(msg_data)
    }
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC FUNCTIONS
////////////////////////////////////////////////////////////////////////////////

pub async fn evaluate_condition(
    infix: &Infix,
    expr1: &Expr,
    expr2: &Expr,
    data: &mut Data<'_>,
    msg_data: &mut MessageData,
    sender: &Option<mpsc::Sender<MSG>>,
) -> Result<Literal, ErrorInfo> {
    let flow_name = if let ContextStepInfo::InsertedStep { step: _, ref flow } = data.context.step {
        flow.clone()
    } else {
        data.context.flow.clone()
    };

    match (expr1, expr2) {
        (Expr::InfixExpr(i1, ex1, ex2), Expr::InfixExpr(i2, exp_1, exp_2)) => {
            let lhs = Box::pin(evaluate_condition(i1, ex1, ex2, data, msg_data, sender)).await;
            let rhs = Box::pin(evaluate_condition(i2, exp_1, exp_2, data, msg_data, sender)).await;
            evaluate_infix(&flow_name, infix, lhs, rhs)
        }
        (Expr::InfixExpr(i1, ex1, ex2), exp) => {
            let lhs = Box::pin(evaluate_condition(i1, ex1, ex2, data, msg_data, sender)).await;
            let rhs =
                expr_to_literal(exp, &DisplayWarnings::Off, None, data, msg_data, sender).await;
            evaluate_infix(&flow_name, infix, lhs, rhs)
        }
        (exp, Expr::InfixExpr(i1, ex1, ex2)) => {
            let lhs =
                expr_to_literal(exp, &DisplayWarnings::Off, None, data, msg_data, sender).await;
            let rhs = Box::pin(evaluate_condition(i1, ex1, ex2, data, msg_data, sender)).await;
            evaluate_infix(&flow_name, infix, lhs, rhs)
        }
        (exp_1, exp_2) => {
            let lhs =
                expr_to_literal(exp_1, &DisplayWarnings::Off, None, data, msg_data, sender).await;
            let rhs =
                expr_to_literal(exp_2, &DisplayWarnings::Off, None, data, msg_data, sender).await;
            evaluate_infix(&flow_name, infix, lhs, rhs)
        }
    }
}

pub async fn solve_if_statement(
    statement: &IfStatement,
    mut msg_data: MessageData,
    data: &mut Data<'_>,
    instruction_info: &InstructionInfo,
    sender: &Option<mpsc::Sender<MSG>>,
) -> Result<MessageData, ErrorInfo> {
    match statement {
        IfStatement::IfStmt {
            cond,
            consequence: scope,
            then_branch,
            last_action_index,
        } => {
            match &data.context.hold {
                Some(hold) => {
                    if hold.index.command_index <= *last_action_index {
                        msg_data =
                            msg_data + Box::pin(interpret_scope(scope, data, sender)).await?;
                    } else if let Some(then_branch) = then_branch {
                        return Box::pin(solve_if_statement(
                            then_branch,
                            msg_data,
                            data,
                            instruction_info,
                            sender,
                        ))
                        .await;
                    }
                }
                None => {
                    return Box::pin(evaluate_if_condition(
                        cond,
                        msg_data,
                        data,
                        scope,
                        instruction_info,
                        sender,
                        then_branch,
                    ))
                    .await;
                }
            }
            Ok(msg_data)
        }
        IfStatement::ElseStmt(consequence, ..) => {
            msg_data = msg_data + Box::pin(interpret_scope(consequence, data, sender)).await?;
            Ok(msg_data)
        }
    }
}
