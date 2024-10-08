use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Memory::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Memory::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Memory::BotId).string().not_null())
                    .col(ColumnDef::new(Memory::ChannelId).string().not_null())
                    .col(ColumnDef::new(Memory::UserId).string().not_null())
                    .col(ColumnDef::new(Memory::Key).string().not_null())
                    .col(ColumnDef::new(Memory::Value).string().not_null())
                    .col(
                        ColumnDef::new(Memory::CreatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Memory::UpdatedAt)
                            .date_time()
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(ColumnDef::new(Memory::ExpiresAt).date_time())
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();

        db.execute_unprepared(
            "CREATE TRIGGER memory_updated_at
            AFTER UPDATE ON memory
            FOR EACH ROW
            BEGIN
                UPDATE memory
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
            .drop_table(Table::drop().table(Memory::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Memory {
    Table,
    Id,
    BotId,
    ChannelId,
    UserId,
    Key,
    Value,
    CreatedAt,
    UpdatedAt,
    ExpiresAt,
}
