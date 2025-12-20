mod covers;
mod creators;
mod error;
mod paginator;
mod publist;
mod refs;
pub mod search;
mod titles;
mod yearsummary;

pub use self::creators::CoverSet;
pub use self::paginator::Paginator;
pub use self::publist::{OtherContribs, PartsPublished};
pub use self::yearsummary::ContentSummary;

use self::covers::{cover_image, redirect_cover};
use self::error::{ViewError, ViewResult, for_rejection};
use self::search::{search, search_autocomplete};
use crate::DbOpt;
use crate::dbopt::PgPool;
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
use crate::templates::{
    Html, RenderRucte, ToHtml, frontpage_html, issue_html, year_html,
};
use bytes::Bytes;
use chrono::{Duration, Utc};
use diesel::dsl::{count, max, min, not};
use diesel::prelude::*;
use diesel::result::Error as DbError;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use mime::TEXT_PLAIN;
use regex::Regex;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::sync::OnceLock;
use tokio::net::TcpListener;
use tracing::info;
use warp::filters::BoxedFilter;
use warp::http::header::{CONTENT_TYPE, EXPIRES};
use warp::http::response::Builder;
use warp::http::status::StatusCode;
use warp::path::Tail;
use warp::reply::Response;
use warp::{self, Filter, Reply};

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    db: DbOpt,

    /// Adress to listen on
    #[clap(long, default_value = "127.0.0.1:1536")]
    bind: SocketAddr,
}

type PgFilter = BoxedFilter<(PgPool,)>;

/// Get or head - a filter matching GET and HEAD requests only.
fn goh() -> BoxedFilter<()> {
    use warp::{get, head};
    get().or(head()).unify().boxed()
}

impl Args {
    pub async fn run(&self) -> anyhow::Result<()> {
        use warp::filters::query::query;
        use warp::{path, path::end, path::param, path::tail};
        let pool = self.db.get_pool().unwrap();
        let s = warp::any().map(move || pool.clone()).boxed();
        let s = move || s.clone();
        let routes = warp::any()
            .and(path("s").and(tail()).and(goh()).then(static_file).map(wrap))
            .or(path("c")
                .and(param())
                .and(end())
                .and(goh())
                .and(s())
                .then(cover_image)
                .map(wrap))
            .or(end().and(goh()).and(s()).then(frontpage).map(wrap))
            .or(path("search")
                .and(end())
                .and(query())
                .and(goh())
                .and(s())
                .then(search)
                .map(wrap))
            .or(path("ac")
                .and(end())
                .and(query())
                .and(goh())
                .and(s())
                .then(search_autocomplete)
                .map(wrap))
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
                .then(redirect_cover)
                .map(wrap))
            .or(path("robots.txt")
                .and(end())
                .and(goh())
                .then(robots_txt)
                .map(wrap))
            .or(param()
                .and(end())
                .and(goh())
                .and(s())
                .then(yearsummary::year_summary)
                .map(wrap))
            .or(param()
                .and(param())
                .and(end())
                .and(goh())
                .and(s())
                .then(issue)
                .map(wrap))
            .or(param()
                .and(path("details"))
                .and(end())
                .and(goh())
                .and(s())
                .then(list_year)
                .map(wrap))
            .or(param()
                .and(end())
                .and(goh())
                .and(s())
                .then(titles::oldslug)
                .map(wrap))
            .recover(for_rejection);

        let acceptor = TcpListener::bind(self.bind).await?;
        if let Ok(addr) = acceptor.local_addr() {
            info!("Running on http://{addr}/");
        }
        warp::serve(routes).incoming(acceptor).run().await;
        Ok(())
    }
}

type Result<T, E = ViewError> = std::result::Result<T, E>;

fn wrap(result: Result<impl Reply>) -> Response {
    match result {
        Ok(reply) => reply.into_response(),
        Err(err) => err.into_response(),
    }
}

/// Handler for static files.
/// Create a response from the file data with a correct content type
/// and a far expires header (or a 404 if the file does not exist).
#[allow(clippy::needless_pass_by_value)]
async fn static_file(name: Tail) -> Result<Response> {
    use crate::templates::statics::StaticFile;
    if let Some(data) = StaticFile::get(name.as_str()) {
        let far_expires = Utc::now() + Duration::days(180);
        Builder::new()
            .header(CONTENT_TYPE, data.mime.as_ref())
            .header(EXPIRES, far_expires.to_rfc2822())
            // TODO: Remove `bytes` dep when seanmonstar/warp#1144 is released.
            .body(Bytes::from(data.content).into())
            .ise()
    } else {
        info!(?name, "Static file not found");
        Err(ViewError::NotFound)
    }
}

async fn robots_txt() -> Result<impl Reply> {
    Ok(Builder::new()
        .header(CONTENT_TYPE, TEXT_PLAIN.as_ref())
        .body("User-agent: *\nDisallow: /search\nDisallow: /ac\n"))
}

async fn frontpage(pool: PgPool) -> Result<impl Reply> {
    let mut db = pool.get().await?;

    let (n, of_n): (i64, Option<i32>) = i::issues
        .select((diesel::dsl::count(i::id), max(i::ord)))
        .first(&mut db)
        .await?;

    let of_n = of_n.map(Into::into).unwrap_or(n);

    let n = p::publications
        .select(count(p::issue_id).aggregate_distinct())
        .filter(not(p::seqno.is_null()))
        .first(&mut db)
        .await?;

    let years = i::issues
        .select(i::year)
        .distinct()
        .order(i::year)
        .load(&mut db)
        .await?;

    let all_fa = refs::get_all_fa(&mut db).await?;

    let num = 50;
    let titles = Title::cloud(num, &mut db).await?;
    let refkeys = RefKey::cloud(num, &mut db).await?;
    let creators = Creator::cloud(num, &mut db).await?;

    Ok(Builder::new().html(|o| {
        frontpage_html(
            o, n, of_n, &all_fa, &years, &titles, &refkeys, &creators,
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
    async fn load_details(
        episode: Episode,
        db: &mut AsyncPgConnection,
    ) -> Result<FullEpisode, DbError> {
        let refs = RefKeySet::for_episode(&episode, db).await?;
        let creators = CreatorSet::for_episode(&episode, db).await?;
        let published = PartsPublished::for_episode(&episode, db).await?;
        let orig_mag = episode.load_orig_mag(db).await?;
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
        db: &mut AsyncPgConnection,
    ) -> Result<FullEpisode, DbError> {
        let refs = RefKeySet::for_episode(&episode, db).await?;
        let creators = CreatorSet::for_episode(&episode, db).await?;
        let published =
            PartsPublished::for_episode_except(&episode, issue, db).await?;
        let orig_mag = episode.load_orig_mag(db).await?;
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
    async fn load(
        article: Article,
        db: &mut AsyncPgConnection,
    ) -> Result<FullArticle, DbError> {
        let refs = RefKeySet::for_article(&article, db).await?;
        let creators = CreatorSet::for_article(&article, db).await?;
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
    static FA: OnceLock<Regex> = OnceLock::new();
    static URL: OnceLock<Regex> = OnceLock::new();
    let fa = FA.get_or_init(|| {
        Regex::new(r"\b[Ff]a (?P<ii>(?P<i>[1-9]\d?)(-[1-9]\d?)?)[ /](?P<y>(19|20)\d{2})\b")
            .unwrap()
    });
    let url = URL.get_or_init(|| {
        Regex::new(r"\b(?P<p>https?)://(?P<l>[a-z0-9?%./=&;-]+)").unwrap()
    });
    let html = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let html = fa.replace_all(&html, "<a href='/$y/$i'>Fa $ii/$y</a>");
    let html = url.replace_all(&html, "<a href='$p://$l'>$l</a>");
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

async fn issue(year: i16, issue: u8, db: PgPool) -> Result<impl Reply> {
    let mut db = db.get().await?;
    let issue: Issue = i::issues
        .filter(i::year.eq(year))
        .filter(i::number.eq(i16::from(issue)))
        .first(&mut db)
        .await
        .optional()?
        .ok_or(ViewError::NotFound)?;

    let pubyear = i::issues
        .select((i::year, (i::number, i::number_str)))
        .filter(i::year.eq(issue.year))
        .order(i::number)
        .load::<IssueRef>(&mut db)
        .await?;

    let details = IssueDetails::load_full(issue, &mut db).await?;
    let years = YearLinks::load(year, &mut db).await?.link_current();
    Ok(Builder::new().html(|o| issue_html(o, &years, &details, &pubyear))?)
}

async fn list_year(year: i16, db: PgPool) -> Result<impl Reply> {
    let mut db = db.get().await?;
    let issues_in = i::issues
        .filter(i::year.eq(year))
        .order(i::number)
        .load(&mut db)
        .await?;
    if issues_in.is_empty() {
        return Err(ViewError::NotFound);
    }
    let mut issues = Vec::with_capacity(issues_in.len());
    for issue in issues_in {
        issues.push(IssueDetails::load_full(issue, &mut db).await?);
    }
    let years = YearLinks::load(year, &mut db).await?;
    Ok(Builder::new().html(|o| year_html(o, year, &years, &issues))?)
}

pub struct IssueDetails {
    pub issue: Issue,
    pub cover_by: Vec<Creator>,
    pub contents: Vec<PublishedInfo>,
}

impl IssueDetails {
    async fn load_full(
        issue: Issue,
        db: &mut AsyncPgConnection,
    ) -> Result<IssueDetails, DbError> {
        let cover_by = cover_by(issue.id, db).await?;

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
            .filter(p::issue_id.eq(issue.id))
            .order(p::seqno)
            .load::<(
                Option<(Title, Episode, Part)>,
                Option<Article>,
                Option<i16>,
                Option<i16>,
                String,
            )>(db)
            .await?;
        let mut contents = Vec::with_capacity(content_raw.len());
        for row in content_raw {
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
                        episode: FullEpisode::in_issue(e, &issue, db).await?,
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
                            FullArticle::load(a, db).await?,
                        ),
                        seqno,
                        classnames: "article",
                    });
                }
                row => panic!("Strange row: {row:?}"),
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
    db: &mut AsyncPgConnection,
) -> Result<Vec<Creator>, DbError> {
    c::creators
        .inner_join(ca::creator_aliases.inner_join(cb::covers_by))
        .select((c::id, ca::name, c::slug))
        .filter(cb::issue_id.eq(issue_id))
        .load(db)
        .await
}

fn redirect(url: &str) -> Result<Response> {
    use warp::http::header::LOCATION;
    let msg = format!("Try {url:?}");
    Builder::new()
        .status(StatusCode::PERMANENT_REDIRECT)
        .header(LOCATION, url)
        .body(msg.into())
        .ise()
}

pub struct YearLinks {
    first: i16,
    shown: i16,
    last: i16,
    link_current: bool,
}

impl YearLinks {
    async fn load(year: i16, db: &mut AsyncPgConnection) -> Result<Self> {
        let (first, last) = i::issues
            .select((min(i::year), max(i::year)))
            .first::<(Option<i16>, Option<i16>)>(db)
            .await?;
        let y = |y: Option<i16>| -> i16 { y.unwrap_or(year) };
        Ok(YearLinks::new(y(first), year, y(last)))
    }
    fn new(first: i16, shown: i16, last: i16) -> Self {
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
        let one = |out: &mut dyn Write, y: i16| -> io::Result<()> {
            if y == shown {
                if self.link_current {
                    write!(out, "<a href='/{y}'><b>{y}</b></a>")?;
                } else {
                    write!(out, "<b>{y}</b>")?;
                }
            } else {
                write!(out, "<a href='/{y}'>{y}</a>")?;
            }
            Ok(())
        };
        one(out, self.first)?;
        let mut skip = false;
        for y in self.first + 1..=self.last {
            if y % 10 == 0 || y.abs_diff(shown) < 3 || y == self.last {
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
