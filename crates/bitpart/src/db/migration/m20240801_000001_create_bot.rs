use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Bot::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Bot::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Bot::BotId).string().not_null())
                    .col(ColumnDef::new(Bot::Bot).string().not_null())
                    .col(ColumnDef::new(Bot::EngineVersion).string().not_null())
                    .col(
                        ColumnDef::new(Bot::UpdatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Bot::CreatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();

        db.execute_unprepared(
            "CREATE TRIGGER bot_updated_at
            AFTER UPDATE ON bot
            FOR EACH ROW
            BEGIN
                UPDATE bot
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Bot::Table).to_owned())
            .await
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(DeriveIden)]
enum Bot {
    Table,
    Id,
    BotId,
    Bot,
    EngineVersion,
    UpdatedAt,
    CreatedAt,
}
