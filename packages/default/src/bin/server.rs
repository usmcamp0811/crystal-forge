use axum::{Router, routing::get};
use crystal_forge::config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::load_config()?;
    let db_url = config.to_url();

    config::validate_db_connection(&db_url).await?;
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(handle_post));

    // run our app with hyper, listening globally on port 3000

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn handle_post(body: String) -> String {
    format!("Got: {}", body)
}
