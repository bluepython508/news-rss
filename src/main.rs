use anyhow::*;
use axum::{handler::get, http::StatusCode, Router};
use futures::future::try_join_all;
use news_rss::{Article, Scraper, RTE};
use rss::{ChannelBuilder, GuidBuilder, ItemBuilder};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{runtime::Handle, select, sync::Mutex, time::sleep};
use tracing::{instrument, span, trace, Instrument, Level};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .finish(),
    )?;
    let feeds = Arc::new(Mutex::new(HashMap::new()));
    select!(
        r = tokio::spawn(server(feeds.clone())) => r,
        r = tokio::task::spawn_blocking(|| Handle::current().block_on(scrape(&[RTE], feeds))) => r
    )??;
    Ok(())
}

#[instrument(skip(feeds))]
async fn server(feeds: Arc<Mutex<HashMap<&'static str, Vec<Article>>>>) -> Result<()> {
    let feed = |name: &'static str| {
        move || {
            async move {
                trace!("Entered feed handler");
                let feeds = feeds.lock().await;
                let feed = feeds.get(name);
                let feed = if let Some(feed) = feed {
                    feed
                } else {
                    trace!("Feed not found");
                    return Err(StatusCode::NOT_FOUND);
                };
                let items = feed
                    .iter()
                    .map(|article| {
                        ItemBuilder::default()
                            .title(article.headline.to_owned())
                            .guid(
                                GuidBuilder::default()
                                    .value(article.link.as_str().to_owned())
                                    .permalink(true)
                                    .build()
                                    .unwrap(),
                            )
                            .link(article.link.as_str().to_owned())
                            .pub_date(article.date.to_rfc2822())
                            .content(article.body.to_owned())
                            .build()
                            .unwrap()
                    })
                    .collect::<Vec<_>>();
                Ok(ChannelBuilder::default()
                    .title(name)
                    .items(items)
                    .build()
                    .unwrap()
                    .to_string())
            }
            .instrument(span!(
                Level::TRACE,
                "feed-handler",
                name
            ))
        }
    };
    let app = Router::new().route("/rte.rss", get(feed("RTE")));

    let address = SocketAddr::new("0.0.0.0".parse().unwrap(), 3000);
    axum::Server::bind(&address)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

#[instrument(skip(out))]
async fn scrape(
    feeds: &[Scraper],
    out: Arc<Mutex<HashMap<&'static str, Vec<Article>>>>,
) -> Result<()> {
    let client = reqwest::ClientBuilder::new().build()?;
    loop {
        let articles = try_join_all(feeds.iter().map(|x| x.get_articles(&client))).await?;
        let mut out = out.lock().await;
        for (feed, articles) in feeds.iter().zip(articles) {
            out.insert(feed.name, articles);
        }
        sleep(Duration::from_secs(60 * 60)).await;
    }
}
