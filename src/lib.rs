use anyhow::*;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::{Europe, Tz};
use nipper::{Document, Selection};
use reqwest::{Client, Url};
use std::fmt::Debug;
use tracing::{Level, Span, instrument, span, trace};

#[derive(Debug)]
pub struct Article {
    pub headline: String,
    pub link: Url,
    pub body: String,
    pub image: Option<Url>,
    pub date: DateTime<Tz>,
}

#[derive(Debug)]
pub struct Scraper {
    pub name: &'static str,
    base_url: &'static str,
    news_url: &'static str,
    article_selector: &'static str,
    headline_selector: &'static str,
    image_selector: Option<&'static str>,
    date_selector: &'static str,
    parse_date: fn(String) -> Result<DateTime<Tz>>,
    link_selector: &'static str,
    body_selector: &'static str,
}

impl Scraper {
    #[instrument(skip(self), fields(self.base_url))]
    fn url(&self, path: &str) -> Result<Url> {
        Url::parse(self.base_url)
            .expect("Expected base URL to be valid")
            .join(path)
            .map_err(Into::into)
    }

    #[instrument(skip(self, client), fields(self.name))]
    pub async fn get_articles(&self, client: &Client) -> Result<Vec<Article>> {
        let news = client
            .get(self.url(self.news_url)?)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let news = Document::from(&news);
        let articles = news.select(self.article_selector);
        let articles = futures::future::try_join_all(
            articles
                .iter()
                .map(|article| self.get_article(client, article)),
        ).await?;
        Ok(articles)
    }

    #[instrument(skip(self, client, article), fields(self.name, article))]
    async fn get_article<'a>(&self, client: &Client, article: Selection<'a>) -> Result<Article> {
        let headline = article
            .select(self.headline_selector)
            .text()
            .to_string()
            .trim()
            .to_owned();
        let link = self.url(
            &article
                .select(self.link_selector)
                .attr("href")
                .context("Require article link to have href")?
                .to_string(),
        )?;
        Span::current().record("article", &link.as_str());
        drop(article);
        let document = Document::from(
            &client
                .get(link.clone())
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?,
        );

        let body = document.select(self.body_selector).html().to_string();
        let image = if let Some(sel) = self.image_selector {
            Some(
                document
                    .select(sel)
                    .attr("src")
                    .context("Expect image to have src")?
                    .to_string()
                    .parse()?,
            )
        } else {
            None
        };

        let date = (self.parse_date)(document.select(self.date_selector).text().to_string())?;

        Ok(Article {
            headline,
            link,
            body,
            image,
            date,
        })
    }
}
pub const RTE: Scraper = Scraper {
    name: "RTE",
    base_url: "https://www.rte.ie/",
    news_url: "/news/",
    article_selector: ":not(.av-box) ~ .article-meta",
    headline_selector: "span.underline",
    link_selector: "a",
    body_selector: "section.article-body",
    image_selector: None,
    date_selector: "span.modified-date",
    parse_date: |date| {
        let span = span!(Level::TRACE, "RTE.parse_date", date = date.as_str());
        let _entered = span.enter();
        trace!("Parsing date");
        Ok(Europe::Dublin
            .from_local_datetime(&NaiveDateTime::parse_from_str(
                date.trim(),
                "Updated / %A, %-d %b %Y %R",
            ).unwrap_or(Utc::now().with_timezone(&Europe::Dublin).naive_local()))
            .earliest()
            .context("No local date")?)
    },
};
