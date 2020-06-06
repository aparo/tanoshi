extern crate argon2;
extern crate libloading as lib;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use anyhow::Result;
use rust_embed::RustEmbed;
use sqlx::postgres::PgPool;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use warp::{http::header::HeaderValue, path::Tail, reply::Response, Filter, Rejection, Reply};

mod auth;
mod extension;
mod favorites;
mod filters;
mod handlers;

#[derive(RustEmbed)]
#[folder = "../tanoshi-web/dist/"]
struct Asset;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let secret = std::env::var("TOKEN_SECRET_KEY").unwrap();
    let plugin_path = std::env::var("PLUGIN_PATH").unwrap_or("./plugins".to_string());

    let extensions = Arc::new(RwLock::new(extension::Extensions::new()));
    for entry in std::fs::read_dir(plugin_path)? {
        let entry = entry?;
        let path = entry.path();
        let ext = path
            .extension()
            .unwrap_or("".as_ref())
            .to_str()
            .unwrap_or("");
        if ext == "so" || ext == "dll" || ext == "dylib" {
            info!("load plugin from {:?}", path.clone());
            let mut exts = extensions.write().await;
            unsafe {
                match exts.load(path) {
                    Ok(_) => {}
                    Err(e) => error!("not a valid extensions {}", e),
                }
            }
        }
    }
    let exts = extensions.clone();
    let exts = exts.read().await;
    info!("there are {} plugins", exts.extensions().len());

    let static_files = warp::get().and(warp::path::tail()).and_then(serve);
    let index = warp::get().and_then(serve_index);

    let static_files = static_files.or(index);

    let pool = PgPool::builder()
        .max_size(5) // maximum number of connections in the pool
        .build(std::env::var("DATABASE_URL").unwrap().as_str())
        .await?;

    let auth_api = filters::auth::authentication(secret.clone(), pool.clone());
    let manga_api = filters::manga::manga(secret.clone(), extensions, pool.clone());

    let fav = favorites::Favorites::new();
    let fav_api = filters::favorites::favorites(secret.clone(), fav, pool.clone());

    let history_api = filters::history::history(secret.clone(), pool.clone());

    let updates_api = filters::updates::updates(secret.clone(), pool.clone());

    let api = manga_api
        .or(auth_api)
        .or(fav_api)
        .or(history_api)
        .or(updates_api)
        .recover(filters::handle_rejection);

    let routes = api.or(static_files).with(warp::log("manga"));

    let port = std::env::var("PORT").unwrap_or("80".to_string());
    warp::serve(routes)
        .run(std::net::SocketAddrV4::from_str(format!("0.0.0.0:{}", port).as_str()).unwrap())
        .await;
    Ok(())
}

async fn serve_index() -> Result<impl Reply, Rejection> {
    serve_impl("index.html")
}

async fn serve(path: Tail) -> Result<impl Reply, Rejection> {
    serve_impl(path.as_str())
}

fn serve_impl(path: &str) -> Result<impl Reply, Rejection> {
    let asset = Asset::get(path).ok_or_else(warp::reject::not_found)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();

    let mut res = Response::new(asset.into());
    res.headers_mut().insert(
        "content-type",
        HeaderValue::from_str(mime.as_ref()).unwrap(),
    );
    Ok(res)
}
