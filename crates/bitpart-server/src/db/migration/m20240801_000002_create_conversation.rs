use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Conversation::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Conversation::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Conversation::BotId).string().not_null())
                    .col(ColumnDef::new(Conversation::ChannelId).string().not_null())
                    .col(ColumnDef::new(Conversation::UserId).string().not_null())
                    .col(ColumnDef::new(Conversation::FlowId).string().not_null())
                    .col(ColumnDef::new(Conversation::StepId).string().not_null())
                    .col(ColumnDef::new(Conversation::Status).string().not_null())
                    .col(
                        ColumnDef::new(Conversation::LastInteractionAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Conversation::UpdatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Conversation::CreatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(ColumnDef::new(Conversation::ExpiresAt).date_time())
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();

        db.execute_unprepared(
            "CREATE TRIGGER conversation_updated_at
            AFTER UPDATE ON conversation
            FOR EACH ROW
            BEGIN
                UPDATE conversation
                SET updated_at = (datetime('now','localtime'))
                WHERE id = NEW.id;
            END;",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        // todo!();

        manager
            .drop_table(Table::drop().table(Conversation::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Conversation {
    Table,
    Id,
    BotId,
    ChannelId,
    UserId,
    FlowId,
    StepId,
    Status,
    LastInteractionAt,
    UpdatedAt,
    CreatedAt,
    ExpiresAt,
}
