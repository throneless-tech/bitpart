use sea_orm_migration::prelude::*;

use super::m20240801_000002_create_conversation::Conversation;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Message::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Message::Id).uuid().not_null().primary_key())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-message-conversation_id")
                            .from(Message::Table, Message::ConversationId)
                            .to(Conversation::Table, Conversation::Id),
                    )
                    .col(ColumnDef::new(Message::ConversationId).uuid().not_null())
                    .col(ColumnDef::new(Message::FlowId).string().not_null())
                    .col(ColumnDef::new(Message::StepId).string().not_null())
                    .col(ColumnDef::new(Message::Direction).string().not_null())
                    .col(ColumnDef::new(Message::Payload).string().not_null())
                    .col(ColumnDef::new(Message::ContentType).string().not_null())
                    .col(ColumnDef::new(Message::MessageOrder).integer().not_null())
                    .col(
                        ColumnDef::new(Message::InteractionOrder)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Message::CreatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Message::UpdatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(ColumnDef::new(Message::ExpiresAt).date_time())
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();

        db.execute_unprepared(
            "CREATE TRIGGER message_updated_at
            AFTER UPDATE ON message
            FOR EACH ROW
            BEGIN
                UPDATE message
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
            .drop_table(Table::drop().table(Message::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Message {
    Table,
    Id,
    ConversationId,
    FlowId,
    StepId,
    Direction,
    Payload,
    ContentType,
    MessageOrder,
    InteractionOrder,
    CreatedAt,
    UpdatedAt,
    ExpiresAt,
}
