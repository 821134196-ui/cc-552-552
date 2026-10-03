use doorplate_server::{build_router, db};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:doorplate.db".into());
    let pool = db::init_pool(&db_url).await?;
    db::seed_if_empty(&pool).await?;

    let app = build_router(pool);
    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("写字楼门牌变更系统已启动: http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
