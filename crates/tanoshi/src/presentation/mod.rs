#[cfg(feature = "embed")]
pub mod assets;
pub mod graphql;
pub mod rest;
pub mod token;

use anyhow::anyhow;
use axum::{
    extract::Extension,
    routing::{get, post},
    Router,
};
use graphql::schema::TanoshiSchema;
use rest::image::Image;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

use self::{
    graphql::{graphql_handler, graphql_playground, schema},
    rest::health::health_check,
};
use crate::{
    application::worker::downloads::DownloadSender,
    db::MangaDatabase,
    domain::services::user::UserService,
    infrastructure::{notifier::Notifier, repositories::user::UserRepositoryImpl},
};
use tanoshi_tracker::{AniList, MyAnimeList};
use tanoshi_vm::extension::SourceBus;

pub struct ServerBuilder {
    user_svc: Option<UserService<UserRepositoryImpl>>,
    mangadb: Option<MangaDatabase>,
    ext_manager: Option<SourceBus>,
    download_tx: Option<DownloadSender>,
    notifier: Option<Notifier<UserRepositoryImpl>>,
    mal_client: Option<MyAnimeList>,
    al_client: Option<AniList>,
    enable_playground: bool,
    secret: Option<String>,
}

impl ServerBuilder {
    pub fn new() -> Self {
        Self {
            user_svc: None,
            mangadb: None,
            ext_manager: None,
            download_tx: None,
            notifier: None,
            mal_client: None,
            al_client: None,
            enable_playground: false,
            secret: None,
        }
    }

    pub fn with_user_svc(self, user_svc: UserService<UserRepositoryImpl>) -> Self {
        Self {
            user_svc: Some(user_svc),
            ..self
        }
    }

    pub fn with_mangadb(self, mangadb: MangaDatabase) -> Self {
        Self {
            mangadb: Some(mangadb),
            ..self
        }
    }

    pub fn with_ext_manager(self, ext_manager: SourceBus) -> Self {
        Self {
            ext_manager: Some(ext_manager),
            ..self
        }
    }

    pub fn with_download_tx(self, download_tx: DownloadSender) -> Self {
        Self {
            download_tx: Some(download_tx),
            ..self
        }
    }

    pub fn with_notifier(self, notifier: Notifier<UserRepositoryImpl>) -> Self {
        Self {
            notifier: Some(notifier),
            ..self
        }
    }

    pub fn with_mal_client(self, mal_client: MyAnimeList) -> Self {
        Self {
            mal_client: Some(mal_client),
            ..self
        }
    }

    pub fn with_anilist_client(self, al_client: AniList) -> Self {
        Self {
            al_client: Some(al_client),
            ..self
        }
    }

    pub fn enable_playground(self) -> Self {
        Self {
            enable_playground: true,
            ..self
        }
    }

    pub fn with_secret(self, secret: String) -> Self {
        Self {
            secret: Some(secret),
            ..self
        }
    }

    pub fn build(self) -> Result<Server, anyhow::Error> {
        let user_svc = self.user_svc.ok_or_else(|| anyhow!("no user service"))?;
        let mangadb = self.mangadb.ok_or_else(|| anyhow!("no manga database"))?;
        let extension_manager = self
            .ext_manager
            .ok_or_else(|| anyhow!("no extension manager"))?;
        let download_tx = self
            .download_tx
            .ok_or_else(|| anyhow!("no download sender"))?;
        let notifier = self.notifier.ok_or_else(|| anyhow!("no notifier"))?;
        let mal_client = self.mal_client;
        let al_client = self.al_client;

        let schema = schema::build(
            user_svc,
            mangadb,
            extension_manager,
            download_tx,
            notifier,
            mal_client,
            al_client,
        );

        let enable_playground = self.enable_playground;
        let secret = self.secret.ok_or_else(|| anyhow!("no secret"))?;

        let image = Image::new(secret);

        Ok(Server::new(enable_playground, schema, image))
    }
}

pub struct Server {
    router: Router<axum::body::Body>,
}

impl Server {
    pub fn new(enable_playground: bool, schema: TanoshiSchema, image: Image) -> Self {
        let mut router = Router::new();

        router = router
            .route("/health", get(health_check))
            .route("/image/:url", get(Image::image))
            .layer(Extension(image));

        if enable_playground {
            router = router
                .route("/graphql", get(graphql_playground).post(graphql_handler))
                .route("/graphql/", post(graphql_handler));
        } else {
            router = router
                .route("/graphql", post(graphql_handler))
                .route("/graphql/", post(graphql_handler));
        }

        router = router.layer(Extension(schema)).layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
                .allow_credentials(true),
        );

        #[cfg(feature = "embed")]
        {
            router = router.fallback(get(assets::static_handler));
        }

        Self { router }
    }

    pub async fn serve<A: Into<SocketAddr>>(self, addr: A) -> Result<(), anyhow::Error> {
        axum::Server::bind(&addr.into())
            .serve(self.router.into_make_service())
            .await?;

        Ok(())
    }
}