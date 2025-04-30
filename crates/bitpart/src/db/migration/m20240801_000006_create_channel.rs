use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Channel::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Channel::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Channel::BotId).string().not_null())
                    .col(ColumnDef::new(Channel::ChannelId).string().not_null())
                    .col(
                        ColumnDef::new(Channel::CreatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Channel::UpdatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();

        db.execute_unprepared(
            "CREATE TRIGGER channel_updated_at
            AFTER UPDATE ON channel
            FOR EACH ROW
            BEGIN
                UPDATE channel
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Channel::Table).to_owned())
            .await
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(DeriveIden)]
enum Channel {
    Table,
    Id,
    BotId,
    ChannelId,
    CreatedAt,
    UpdatedAt,
}
