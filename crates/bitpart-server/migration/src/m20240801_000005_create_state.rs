use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(State::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(State::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(State::BotId).string().not_null())
                    .col(ColumnDef::new(State::ChannelId).string().not_null())
                    .col(ColumnDef::new(State::UserId).string().not_null())
                    .col(ColumnDef::new(State::Type).string().not_null())
                    .col(ColumnDef::new(State::Key).string().not_null())
                    .col(ColumnDef::new(State::Value).string().not_null())
                    .col(ColumnDef::new(State::CreatedAt).date_time().not_null())
                    .col(ColumnDef::new(State::UpdatedAt).date_time().not_null())
                    .col(ColumnDef::new(State::ExpiresAt).date_time())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(State::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum State {
    Table,
    Id,
    BotId,
    ChannelId,
    UserId,
    Type,
    Key,
    Value,
    CreatedAt,
    UpdatedAt,
    ExpiresAt,
}
