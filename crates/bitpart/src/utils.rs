#[cfg(test)]
use crate::api::ApiState;
#[cfg(test)]
use crate::channels::signal;
#[cfg(test)]
use crate::db;
#[cfg(test)]
use axum::Router;
#[cfg(test)]
use axum_test::TestServer;
#[cfg(test)]
use sea_orm::Database;
#[cfg(test)]
use sea_orm_migration::MigratorTrait;

#[cfg(test)]
pub async fn get_test_server(app: Router<ApiState>) -> TestServer {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    db::migration::Migrator::refresh(&db).await.unwrap();

    let state = ApiState {
        db,
        auth: "test".into(),
        manager: signal::SignalManager::new(),
    };
    TestServer::builder()
        .http_transport()
        //.expect_success_by_default()
        .mock_transport()
        .build(app.with_state(state))
        .unwrap()
}
