mod covers;
mod creators;
mod paginator;
mod publist;
mod refs;
pub mod search;
mod titles;
mod yearsummary;

use self::covers::{cover_image, redirect_cover};
pub use self::creators::CoverSet;
pub use self::paginator::Paginator;
pub use self::publist::{OtherContribs, PartsPublished};
use self::search::{search, search_autocomplete};
pub use yearsummary::ContentSummary;

use crate::models::{
    Article, Creator, CreatorSet, Episode, Issue, IssueRef, OtherMag, Part,
    RefKey, RefKeySet, Title,
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
use std::io::{self, Write};
use std::net::SocketAddr;
use structopt::StructOpt;
use tokio_diesel::{AsyncError, AsyncRunQueryDsl};
use warp::filters::BoxedFilter;
use warp::http::header::{CONTENT_TYPE, EXPIRES};
use warp::http::response::Builder;
use warp::http::status::StatusCode;
use warp::path::Tail;
use warp::reply::Response;
use warp::{self, reject::not_found, Filter, Rejection, Reply};

#[derive(StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    db: DbOpt,

    /// Adress to listen on
    #[structopt(long, default_value = "127.0.0.1:1536")]
    bind: SocketAddr,
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
            .and(path("s").and(tail()).and(goh()).and_then(static_file))
            .or(path("c")
                .and(param())
                .and(end())
                .and(goh())
                .and(s())
                .and_then(cover_image))
            .or(end().and(goh()).and(s()).and_then(frontpage))
            .or(path("search")
                .and(end())
                .and(query())
                .and(goh())
                .and(s())
                .and_then(search))
            .or(path("ac")
                .and(end())
                .and(query())
                .and(goh())
                .and(s())
                .and_then(search_autocomplete))
            .or(path("titles").and(titles::routes(s())))
            .or(path("fa").and(refs::fa_route(s())))
            .or(path("what").and(refs::what_routes(s())))
            .or(path("who").and(creators::routes(s())))
            .or(path("static")
                .and(param())
                .and(param())
                .and(end())
                .and(goh())
                .and(s())
                .and_then(redirect_cover))
            .or(path("robots.txt")
                .and(end())
                .and(goh())
                .and_then(robots_txt))
            .or(param()
                .and(end())
                .and(goh())
                .and(s())
                .and_then(yearsummary::year_summary))
            .or(param()
                .and(param())
                .and(end())
                .and(goh())
                .and(s())
                .and_then(issue))
            .or(param()
                .and(path("details"))
                .and(end())
                .and(goh())
                .and(s())
                .and_then(list_year))
            .or(param()
                .and(end())
                .and(goh())
                .and(s())
                .and_then(titles::oldslug))
            .recover(customize_error);
        warp::serve(routes).run(self.bind).await;
        Ok(())
    }
}

/// Handler for static files.
/// Create a response from the file data with a correct content type
/// and a far expires header (or a 404 if the file does not exist).
#[allow(clippy::needless_pass_by_value)]
async fn static_file(name: Tail) -> Result<impl Reply, Rejection> {
    use crate::templates::statics::StaticFile;
    if let Some(data) = StaticFile::get(name.as_str()) {
        let far_expires = Utc::now() + Duration::days(180);
        Ok(Builder::new()
            .header(CONTENT_TYPE, data.mime.as_ref())
            .header(EXPIRES, far_expires.to_rfc2822())
            .body(data.content))
    } else {
        log::info!("Static file {:?} not found", name);
        Err(not_found())
    }
}

async fn robots_txt() -> Result<impl Reply, Rejection> {
    Ok(Builder::new()
        .header(CONTENT_TYPE, TEXT_PLAIN.as_ref())
        .body("User-agent: *\nDisallow: /search\nDisallow: /ac\n"))
}

async fn frontpage(db: PgPool) -> Result<impl Reply, Rejection> {
    let n = p::publications
        .select(sql("count(distinct issue)"))
        .filter(not(p::seqno.is_null()))
        .first_async(&db)
        .await
        .map_err(custom)?;

    let years = i::issues
        .select(i::year)
        .distinct()
        .order(i::year)
        .load_async(&db)
        .await
        .map_err(custom)?;

    let all_fa = refs::get_all_fa(&db).await.map_err(custom)?;

    let num = 50;
    let titles = Title::cloud(num, &db).await.map_err(custom)?;
    let refkeys = RefKey::cloud(num, &db).await.map_err(custom)?;
    let creators = Creator::cloud(num, &db).await.map_err(custom)?;

    Builder::new().html(|o| {
        templates::frontpage(
            o, n, &all_fa, &years, &titles, &refkeys, &creators,
        )
    })
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
    let html = FA.replace_all(&html, "<a href='/$y/$i'>Fa $ii/$y</a>");
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
        "See <a href='/1980/7'>Fa 7/1980</a>.",
    )
}
#[test]
fn text_to_fa_html_d() {
    assert_eq!(
        text_to_fa_html("See Fa 25-26/2019.").0,
        "See <a href='/2019/25'>Fa 25-26/2019</a>.",
    )
}

#[test]
fn text_to_fa_html_e() {
    assert_eq!(
        text_to_fa_html("See https://rasmus.krats.se .").0,
        "See <a href='https://rasmus.krats.se'>rasmus.krats.se</a> .",
    )
}

async fn issue(
    year: u16,
    issue: u8,
    db: PgPool,
) -> Result<impl Reply, Rejection> {
    let issue: Issue = i::issues
        .filter(i::year.eq(year as i16))
        .filter(i::number.eq(i16::from(issue)))
        .first_async(&db)
        .await
        .map_err(custom_or_404)?;

    let pubyear = i::issues
        .select((i::year, (i::number, i::number_str)))
        .filter(i::year.eq(issue.year))
        .order(i::number)
        .load_async::<IssueRef>(&db)
        .await
        .map_err(custom)?;

    let details =
        IssueDetails::load_full(issue, &db).await.map_err(custom)?;
    let years = YearLinks::load(year, db).await?.link_current();
    Builder::new().html(|o| templates::issue(o, &years, &details, &pubyear))
}

async fn list_year(year: u16, db: PgPool) -> Result<impl Reply, Rejection> {
    use futures::stream::{self, StreamExt, TryStreamExt};
    let issues = i::issues
        .filter(i::year.eq(year as i16))
        .order(i::number)
        .load_async(&db)
        .await
        .map_err(custom)?;
    if issues.is_empty() {
        return Err(not_found());
    }
    let issues = stream::iter(issues)
        .then(|issue| IssueDetails::load_full(issue, &db))
        .try_collect::<Vec<_>>()
        .await
        .map_err(custom)?;
    let years = YearLinks::load(year, db).await?;
    Builder::new().html(|o| templates::year(o, year, &years, &issues))
}

pub struct IssueDetails {
    pub issue: Issue,
    pub cover_by: Vec<Creator>,
    pub contents: Vec<PublishedInfo>,
}

impl IssueDetails {
    async fn load_full(
        issue: Issue,
        db: &PgPool,
    ) -> Result<IssueDetails, AsyncError> {
        let cover_by = cover_by(issue.id, &db).await?;

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
                    (ep::part_no, ep::part_name),
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
        Ok(IssueDetails {
            issue,
            cover_by,
            contents,
        })
    }
    pub fn description(&self) -> String {
        let mut result = format!("Innehållet i Fantomen {}.", self.issue);
        for c in &self.contents {
            if let PublishedContent::EpisodePart { title, .. } = &c.content {
                result.push(' ');
                result.push_str(&title.title);
                result.push('.');
            }
        }
        result
    }
}

async fn cover_by(
    issue_id: i32,
    db: &PgPool,
) -> Result<Vec<Creator>, AsyncError> {
    c::creators
        .inner_join(ca::creator_aliases.inner_join(cb::covers_by))
        .select((c::id, ca::name, c::slug))
        .filter(cb::issue_id.eq(issue_id))
        .load_async(&db)
        .await
}

fn custom_or_404(e: AsyncError) -> Rejection {
    match e {
        AsyncError::Error(diesel::result::Error::NotFound) => not_found(),
        e => custom(e),
    }
}

fn redirect(url: &str) -> Result<Response, Rejection> {
    use warp::http::header::LOCATION;
    let msg = format!("Try {:?}", url);
    Builder::new()
        .status(StatusCode::PERMANENT_REDIRECT)
        .header(LOCATION, url)
        .body(msg.into())
        .map_err(custom)
}

async fn customize_error(err: Rejection) -> Result<impl Reply, Rejection> {
    if err.is_not_found() {
        log::debug!("Got a 404: {:?}", err);
        Builder::new()
            .status(StatusCode::NOT_FOUND)
            .html(|o| templates::notfound(o, StatusCode::NOT_FOUND))
    } else {
        if let Some(ise) = err.find::<ISE>() {
            log::error!("Internal server error: {}", ise.0);
        } else {
            log::error!("Internal server error: {:?}", err);
        }
        let code = StatusCode::INTERNAL_SERVER_ERROR; // FIXME
        Builder::new()
            .status(code)
            .html(|o| templates::error(o, code))
    }
}

pub struct YearLinks {
    first: u16,
    shown: u16,
    last: u16,
    link_current: bool,
}

impl YearLinks {
    async fn load(year: u16, db: PgPool) -> Result<Self, Rejection> {
        let (first, last) = i::issues
            .select((
                sql::<SmallInt>("min(year)"),
                sql::<SmallInt>("max(year)"),
            ))
            .first_async::<(i16, i16)>(&db)
            .await
            .map_err(custom)?;
        Ok(YearLinks::new(first as u16, year, last as u16))
    }
    fn new(first: u16, shown: u16, last: u16) -> Self {
        YearLinks {
            first,
            shown,
            last,
            link_current: false,
        }
    }
    fn link_current(mut self) -> Self {
        self.link_current = true;
        self
    }
}

impl ToHtml for YearLinks {
    fn to_html(&self, out: &mut dyn Write) -> io::Result<()> {
        let shown = self.shown;
        let one = |out: &mut dyn Write, y: u16| -> io::Result<()> {
            if y == shown {
                if self.link_current {
                    write!(out, "<a href='/{}'><b>{}</b></a>", y, y)?;
                } else {
                    write!(out, "<b>{}</b>", y)?;
                }
            } else {
                write!(out, "<a href='/{}'>{}</a>", y, y)?;
            }
            Ok(())
        };
        one(out, self.first)?;
        let mut skip = false;
        for y in self.first + 1..=self.last {
            if y % 10 == 0
                || (y as i16 - shown as i16).abs() < 3
                || y == self.last
            {
                out.write_all(if skip { "… ".as_bytes() } else { b", " })?;
                one(out, y)?;
                skip = false;
            } else {
                skip = true;
            }
        }
        Ok(())
    }
}

use warp::reject::Reject;
#[derive(Debug)]
struct ISE(String);
impl Reject for ISE {}

fn custom<E: std::fmt::Display + std::fmt::Debug>(e: E) -> Rejection {
    warp::reject::custom(ISE(format!("{}\nDetails: ({:#?})", e, e)))
}
