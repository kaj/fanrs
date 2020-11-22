mod covers;
mod creators;
mod error;
mod paginator;
mod publist;
mod refs;
pub mod search;
mod titles;

use self::covers::{cover_image, redirect_cover};
pub use self::creators::CoverSet;
pub use self::paginator::Paginator;
pub use self::publist::{OtherContribs, PartsPublished};
use self::refs::{get_all_fa, one_fa};
use self::search::{search, search_autocomplete};

use self::error::{OptionalExtension, ServerError};
use crate::models::{
    Article, Creator, CreatorSet, Episode, Issue, OtherMag, Part, RefKey,
    RefKeySet, Title,
};
use crate::schema::articles::dsl as a;
use crate::schema::covers_by::dsl as cb;
use crate::schema::creator_aliases::dsl as ca;
use crate::schema::creators::dsl as c;
use crate::schema::episode_parts::dsl as ep;
use crate::schema::episodes::dsl as e;
use crate::schema::issues::dsl as i;
use crate::schema::publications::dsl as p;
use crate::schema::titles::dsl as t;
use crate::templates::{self, Html, RenderRucte, ToHtml};
use crate::DbOpt;
use chrono::{Duration, Utc};
use diesel::dsl::{not, sql};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PoolError};
use diesel::sql_types::SmallInt;
use diesel::QueryDsl;
use lazy_static::lazy_static;
use mime::TEXT_PLAIN;
use regex::Regex;
use std::convert::Infallible;
use std::io::{self, Write};
use structopt::StructOpt;
use tokio_diesel::{AsyncError, AsyncRunQueryDsl};
use warp::filters::BoxedFilter;
use warp::http::header::{CONTENT_TYPE, EXPIRES};
use warp::http::response::Builder;
use warp::http::status::StatusCode;
use warp::path::Tail;
use warp::reply::Response;
use warp::{self, Filter, Rejection, Reply};

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,
}

pub type PgPool = Pool<ConnectionManager<PgConnection>>;
type PgFilter = BoxedFilter<(PgPool,)>;

/// Get or head - a filter matching GET and HEAD requests only.
fn goh() -> BoxedFilter<()> {
    use warp::{get, head};
    get().or(head()).unify().boxed()
}

impl Args {
    pub async fn run(&self) -> Result<(), PoolError> {
        let pool = self.db.get_pool()?;
        let s = warp::any().map(move || pool.clone()).boxed();
        let s = move || s.clone();
        use warp::filters::query::query;
        use warp::{path, path::end, path::param, path::tail};
        let routes = warp::any()
            .and(goh().and(path("s")).and(tail()).map_async(static_file))
            .or(goh()
                .and(path("c"))
                .and(s())
                .and(param())
                .and(end())
                .map_async(cover_image))
            .or(goh().and(end()).and(s()).map_async(frontpage))
            .or(goh()
                .and(path("search"))
                .and(end())
                .and(s())
                .and(query())
                .map_async(search))
            .or(goh()
                .and(path("ac"))
                .and(end())
                .and(s())
                .and(query())
                .map_async(search_autocomplete))
            .or(path("titles").and(titles::routes(s())))
            .or(goh()
                .and(path("fa"))
                .and(s())
                .and(param())
                .and(end())
                .map_async(one_fa))
            .or(path("what").and(refs::what_routes(s())))
            .or(path("who").and(creators::routes(s())))
            .or(goh()
                .and(path("static"))
                .and(s())
                .and(param())
                .and(param())
                .and(end())
                .map_async(redirect_cover))
            .or(goh()
                .and(path("robots.txt"))
                .and(end())
                .map_async(robots_txt))
            .or(goh().and(s()).and(param()).and(end()).map_async(list_year))
            .or(goh()
                .and(s())
                .and(param())
                .and(end())
                .map_async(titles::oldslug))
            .recover(customize_error);
        warp::serve(routes).run(([127, 0, 0, 1], 1536)).await;
        Ok(())
    }
}

/// Handler for static files.
/// Create a response from the file data with a correct content type
/// and a far expires header (or a 404 if the file does not exist).
#[allow(clippy::needless_pass_by_value)]
async fn static_file(name: Tail) -> Result<impl Reply, ServerError> {
    use crate::templates::statics::StaticFile;
    if let Some(data) = StaticFile::get(name.as_str()) {
        let far_expires = Utc::now() + Duration::days(180);
        Ok(Builder::new()
            .header(CONTENT_TYPE, data.mime.as_ref())
            .header(EXPIRES, far_expires.to_rfc2822())
            .body(data.content))
    } else {
        log::info!("Static file {:?} not found", name);
        Err(ServerError::not_found())
    }
}

async fn robots_txt() -> Result<impl Reply, ServerError> {
    Ok(Builder::new()
        .header(CONTENT_TYPE, TEXT_PLAIN.as_ref())
        .body("User-agent: *\nDisallow: /search\nDisallow: /ac\n"))
}

async fn frontpage(db: PgPool) -> Result<impl Reply, ServerError> {
    let n = p::publications
        .select(sql("count(distinct issue)"))
        .filter(not(p::seqno.is_null()))
        .first_async(&db)
        .await?;

    let years = i::issues
        .select(i::year)
        .distinct()
        .order(i::year)
        .load_async(&db)
        .await?;

    let all_fa = get_all_fa(&db).await?;

    let num = 50;
    let titles = Title::cloud(num, &db).await?;
    let refkeys = RefKey::cloud(num, &db).await?;
    let creators = Creator::cloud(num, &db).await?;

    Ok(Builder::new().html(|o| {
        templates::frontpage(
            o, n, &all_fa, &years, &titles, &refkeys, &creators,
        )
    })?)
}

/// Information about an episode / part or article, as published in an issue.
pub struct PublishedInfo {
    pub content: PublishedContent,
    pub seqno: Option<i16>,
    pub classnames: &'static str,
}

impl PublishedInfo {
    pub fn classnames(&self) -> String {
        match self.content {
            PublishedContent::EpisodePart {
                best_plac: Some(p), ..
            } if p <= 3 => format!("{} best{}", self.classnames, p),
            _ => self.classnames.to_string(),
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum PublishedContent {
    Text(FullArticle),
    EpisodePart {
        title: Title,
        episode: FullEpisode,
        part: Part,
        best_plac: Option<i16>,
        label: String,
    },
}

pub struct FullEpisode {
    pub episode: Episode,
    pub refs: RefKeySet,
    pub creators: CreatorSet,
    pub published: PartsPublished,
    pub orig_mag: Option<OtherMag>,
}

impl FullEpisode {
    fn load_details(
        episode: Episode,
        db: &PgConnection,
    ) -> Result<FullEpisode, diesel::result::Error> {
        let refs = RefKeySet::for_episode(&episode, db)?;
        let creators = CreatorSet::for_episode(&episode, db)?;
        let published = PartsPublished::for_episode(&episode, db)?;
        let orig_mag = episode.load_orig_mag(db)?;
        Ok(FullEpisode {
            episode,
            refs,
            creators,
            published,
            orig_mag,
        })
    }
    async fn load_details_async(
        episode: Episode,
        db: &PgPool,
    ) -> Result<FullEpisode, AsyncError> {
        let refs = RefKeySet::for_episode_async(&episode, db).await?;
        let creators = CreatorSet::for_episode_async(&episode, db).await?;
        let published =
            PartsPublished::for_episode_async(&episode, db).await?;
        let orig_mag = episode.load_orig_mag_async(db).await?;
        Ok(FullEpisode {
            episode,
            refs,
            creators,
            published,
            orig_mag,
        })
    }

    async fn in_issue(
        episode: Episode,
        issue: &Issue,
        db: &PgPool,
    ) -> Result<FullEpisode, AsyncError> {
        let refs = RefKeySet::for_episode_async(&episode, db).await?;
        let creators = CreatorSet::for_episode_async(&episode, db).await?;
        let published =
            PartsPublished::for_episode_except(&episode, issue, db).await?;
        let orig_mag = episode.load_orig_mag_async(db).await?;
        Ok(FullEpisode {
            episode,
            refs,
            creators,
            published,
            orig_mag,
        })
    }

    pub fn note(&self) -> Option<Html<String>> {
        self.episode.note.as_ref().map(|s| text_to_fa_html(s))
    }
    pub fn bestclass(&self) -> &str {
        match self.published.bestplac() {
            Some(1) => "best1",
            Some(2) => "best2",
            Some(3) => "best3",
            _ => "",
        }
    }
}

pub struct FullArticle {
    pub article: Article,
    pub refs: RefKeySet,
    pub creators: CreatorSet,
}

impl FullArticle {
    fn load(
        article: Article,
        db: &PgConnection,
    ) -> Result<FullArticle, diesel::result::Error> {
        let refs = RefKeySet::for_article(&article, &db)?;
        let creators = CreatorSet::for_article(&article, &db)?;
        Ok(FullArticle {
            article,
            refs,
            creators,
        })
    }
    async fn load_async(
        article: Article,
        db: &PgPool,
    ) -> Result<FullArticle, AsyncError> {
        let refs = RefKeySet::for_article_async(&article, &db).await?;
        let creators = CreatorSet::for_article_async(&article, &db).await?;
        Ok(FullArticle {
            article,
            refs,
            creators,
        })
    }

    pub fn note(&self) -> Option<Html<String>> {
        self.article.note.as_ref().map(|s| text_to_fa_html(s))
    }
}

fn text_to_fa_html(text: &str) -> Html<String> {
    lazy_static! {
        static ref FA: Regex =
            Regex::new(r"\b[Ff]a (?P<ii>(?P<i>[1-9]\d?)(-[1-9]\d?)?)[ /](?P<y>(19|20)\d{2})\b")
            .unwrap();
        static ref URL: Regex =
            Regex::new(r"\b(?P<p>https?)://(?P<l>[a-z0-9?%./=&;-]+)").unwrap();
    }
    let html = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let html = FA.replace_all(&html, "<a href='/$y#i$i'>Fa $ii/$y</a>");
    let html = URL.replace_all(&html, "<a href='$p://$l'>$l</a>");
    Html(html.to_string())
}

#[test]
fn text_to_fa_html_a() {
    assert_eq!(
        text_to_fa_html("Hello world of the Phantom").0,
        "Hello world of the Phantom",
    )
}

#[test]
fn text_to_fa_html_b() {
    assert_eq!(
        text_to_fa_html("Hello <Kit & Julie>").0,
        "Hello &lt;Kit &amp; Julie&gt;",
    )
}

#[test]
fn text_to_fa_html_c() {
    assert_eq!(
        text_to_fa_html("See Fa 7 1980.").0,
        "See <a href='/1980#i7'>Fa 7/1980</a>.",
    )
}
#[test]
fn text_to_fa_html_d() {
    assert_eq!(
        text_to_fa_html("See Fa 25-26/2019.").0,
        "See <a href='/2019#i25'>Fa 25-26/2019</a>.",
    )
}

#[test]
fn text_to_fa_html_e() {
    assert_eq!(
        text_to_fa_html("See https://rasmus.krats.se .").0,
        "See <a href='https://rasmus.krats.se'>rasmus.krats.se</a> .",
    )
}

async fn list_year(db: PgPool, year: u16) -> Result<impl Reply, ServerError> {
    let issues_raw: Vec<Issue> = i::issues
        .filter(i::year.eq(year as i16))
        .order(i::number)
        .load_async(&db)
        .await?;
    if issues_raw.is_empty() {
        return Err(ServerError::not_found());
    }
    let mut issues = Vec::with_capacity(issues_raw.len());
    for issue in issues_raw.into_iter() {
        let c_columns = (c::id, ca::name, c::slug);
        let cover_by = c::creators
            .inner_join(ca::creator_aliases.inner_join(cb::covers_by))
            .select(c_columns)
            .filter(cb::issue_id.eq(issue.id))
            .load_async(&db)
            .await?;

        let mut have_main = false;
        let content_raw = p::publications
            .left_outer_join(
                ep::episode_parts
                    .inner_join(e::episodes.inner_join(t::titles)),
            )
            .left_outer_join(a::articles)
            .select((
                (
                    t::titles::all_columns(),
                    e::episodes::all_columns(),
                    (ep::id, ep::part_no, ep::part_name),
                )
                    .nullable(),
                a::articles::all_columns().nullable(),
                p::seqno,
                p::best_plac,
                p::label,
            ))
            .filter(p::issue.eq(issue.id))
            .order(p::seqno)
            .load_async::<(
                Option<(Title, Episode, Part)>,
                Option<Article>,
                Option<i16>,
                Option<i16>,
                String,
            )>(&db)
            .await?;
        let mut contents = Vec::with_capacity(content_raw.len());
        for row in content_raw.into_iter() {
            match row {
                (Some((t, mut e, part)), None, seqno, b, label) => {
                    let classnames = if e.teaser.is_none() || !part.is_first()
                    {
                        e.teaser = None;
                        "episode noteaser"
                    } else if t.title == "Fantomen" && !have_main {
                        have_main = true;
                        "episode main"
                    } else {
                        "episode"
                    };
                    let content = PublishedContent::EpisodePart {
                        title: t,
                        episode: FullEpisode::in_issue(e, &issue, &db)
                            .await?,
                        part,
                        best_plac: b,
                        label,
                    };
                    contents.push(PublishedInfo {
                        content,
                        seqno,
                        classnames,
                    });
                }
                (None, Some(a), seqno, None, _label) => {
                    contents.push(PublishedInfo {
                        content: PublishedContent::Text(
                            FullArticle::load_async(a, &db).await?,
                        ),
                        seqno,
                        classnames: "article",
                    });
                }
                row => panic!("Strange row: {:?}", row),
            }
        }
        issues.push((issue, cover_by, contents));
    }
    let years = i::issues
        .select((sql::<SmallInt>("min(year)"), sql::<SmallInt>("max(year)")))
        .first_async::<(i16, i16)>(&db)
        .await?;
    let years = YearLinks::new(years.0 as u16, year, years.1 as u16);
    Ok(Builder::new().html(|o| templates::year(o, year, &years, &issues))?)
}

fn redirect(url: &str) -> Response {
    use warp::http::header::LOCATION;
    let msg = format!("Try {:?}", url);
    Builder::new()
        .status(StatusCode::PERMANENT_REDIRECT)
        .header(LOCATION, url)
        .body(msg.into())
        .unwrap()
}

async fn customize_error(err: Rejection) -> Result<impl Reply, Infallible> {
    Ok(ServerError::from(err))
}

pub struct YearLinks {
    first: u16,
    shown: u16,
    last: u16,
}

impl YearLinks {
    fn new(first: u16, shown: u16, last: u16) -> Self {
        YearLinks { first, shown, last }
    }
}

impl ToHtml for YearLinks {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        let shown = self.shown;
        let one = |out: &mut dyn Write, y: u16| -> io::Result<()> {
            if y == shown {
                write!(out, "<b>{}</b>", y)?;
            } else {
                write!(out, "<a href='/{}'>{}</a>", y, y)?;
            }
            Ok(())
        };
        let from = if self.shown > self.first + 7 {
            self.shown - 5
        } else {
            self.first
        };
        let to = if self.shown + 7 < self.last {
            self.shown + 5
        } else {
            self.last
        };
        if from > self.first {
            one(out, self.first)?;
            write!(out, " … ")?;
        }
        one(out, from)?;
        for y in from + 1..=to {
            write!(out, ", ")?;
            one(out, y)?;
        }
        if to < self.last {
            write!(out, " … ")?;
            one(out, self.last)?;
        }
        Ok(())
    }
}
