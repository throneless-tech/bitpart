use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Runner::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Runner::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Runner::BotId).string().not_null())
                    .col(ColumnDef::new(Runner::RunnerId).string().not_null())
                    .col(ColumnDef::new(Runner::State).string().not_null())
                    .col(
                        ColumnDef::new(Runner::CreatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Runner::UpdatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();

        db.execute_unprepared(
            "CREATE TRIGGER runner_updated_at
            AFTER UPDATE ON runner
            FOR EACH ROW
            BEGIN
                UPDATE runner
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Runner::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Runner {
    Table,
    Id,
    BotId,
    RunnerId,
    State,
    CreatedAt,
    UpdatedAt,
}
