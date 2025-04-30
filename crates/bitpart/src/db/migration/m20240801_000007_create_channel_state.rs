use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ChannelState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ChannelState::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ChannelState::ChannelId).string().not_null())
                    .col(ColumnDef::new(ChannelState::Tree).string().not_null())
                    .col(ColumnDef::new(ChannelState::Key).string().not_null())
                    .col(ColumnDef::new(ChannelState::Value).string().not_null())
                    .col(
                        ColumnDef::new(ChannelState::CreatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ChannelState::UpdatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();

        db.execute_unprepared(
            "CREATE TRIGGER channel_state_updated_at
            AFTER UPDATE ON channel_state
            FOR EACH ROW
            BEGIN
                UPDATE channel_state
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ChannelState::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ChannelState {
    Table,
    Id,
    ChannelId,
    Tree,
    Key,
    Value,
    CreatedAt,
    UpdatedAt,
}
