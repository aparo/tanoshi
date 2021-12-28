// from https://github.com/tauri-apps/tauri-plugin-localhost

use tanoshi_vm::{prelude::SourceManager, vm::SourceBus};
use tauri::{
  plugin::{Plugin, Result as PluginResult},
  AppHandle, Runtime,
};

use tanoshi::{
  config::{self, GLOBAL_CONFIG},
  db, local, notifier, schema, server, worker,
};

pub struct Server {
  port: u16,
}

impl Server {
  pub fn new() -> Self {
    let port = portpicker::pick_unused_port().unwrap();
    Self { port }
  }
}

impl<R: Runtime> Plugin<R> for Server {
  fn name(&self) -> &'static str {
    "graphql"
  }

  fn initialization_script(&self) -> Option<String> {
    Some(format!("window.__TANOSHI_PORT__ = {};", self.port))
  }

  fn initialize(&mut self, _app: &AppHandle<R>, _config: serde_json::Value) -> PluginResult<()> {
    let port = self.port;

    tauri::async_runtime::spawn(async move {
      let config = GLOBAL_CONFIG.get().unwrap();

      let pool = match db::establish_connection(&config.database_path).await {
        Ok(pool) => pool,
        Err(_) => {
          return;
        }
      };

      let mangadb = db::MangaDatabase::new(pool.clone());
      let userdb = db::UserDatabase::new(pool.clone());

      let extension_manager = SourceManager::new(&config.plugin_path);

      match &config.local_path {
        config::LocalFolders::Single(local_path) => {
          let source = local::Local::new(10000, "Local".to_string(), local_path);
          let bus = SourceBus::new(source);
          extension_manager.insert(bus).await.unwrap();
        }
        config::LocalFolders::Multiple(local_paths) => {
          for (index, local_path) in local_paths.iter().enumerate() {
            // source id starts from 10000
            let index = index + 10000;
            let source = local::Local::new(index as i64, local_path.name.clone(), &local_path.path);
            let bus = SourceBus::new(source);
            extension_manager.insert(bus).await.unwrap();
          }
        }
      }

      let notifier = notifier::Builder::new(userdb.clone()).finish();

      let (download_tx, download_worker_handle) = worker::downloads::start(
        &config.download_path,
        mangadb.clone(),
        extension_manager.clone(),
        notifier.clone(),
      );

      let update_worker_handle = worker::updates::start(
        config.update_interval,
        mangadb.clone(),
        extension_manager.clone(),
        download_tx.clone(),
        notifier.clone(),
      );

      let schema = schema::build(userdb, mangadb, extension_manager, download_tx, notifier);

      let app = server::init_app(config, schema);
      let server_fut = server::serve("127.0.0.1", port, app);

      tokio::select! {
          _ = server_fut => {
              println!("server shutdown");
          }
          _ = update_worker_handle => {
              println!("update worker quit");
          }
          _ = download_worker_handle => {
              println!("download worker quit");
          }
          _ = tokio::signal::ctrl_c() => {
              println!("ctrl+c signal");
          }
      }

      pool.close().await;
    });

    Ok(())
  }
}
