# Changes in fanrs

## Unreleased

* Updated `diesel` to 2.3.5 and `diesel-async` to 0.7.4.
* Updated `roxmltree` to 0.21.1 and `scraper` to 0.25.0.
* Updated `warp` to 0.4.2.
* Updated `reqwest` to 0.13.1.


## Release 0.12.0 (2025-04-07)

* Keep track of ordinal issue numbers (PR #8).
* Updated some frontpage and other links.
* Updated to `diesel` 2.2.7 and use `async-diesel` instead of
  `deadpool-diesel` (PR #7 and further changes).
* Changed some db column names for consistency.
* Improved diagnostics on parse errors in read-files.
* Updated to `reqwest` 0.12.2, `scraper` 0.23.1, `roxmltree` 0.20.0,
  and `ructe` 0.18.2.
* Now using `tracing` rather than `log`.
* Removed `lazy_static` dependency, as std handles that now.
* Updated to Rust edition 2024 and clippy-suggested cleanups.
* Updated github workflow (and added a clippy job).
* Prohibited unsafe code (there was none, so just to be sure).


## Release 0.11.4 (2023-02-12)

* Fixed bug in detecting unused episode parts in read-files, and
  improved structure and time measurment of the cleanups.
* Adopted to phantomwiki updates in fetch-covers.
* Updated ructe to 0.16.1, clap to 4.0.32, env_logger to 0.10.0,
  roxmltree to 0.18.0, and scraper to 0.14.0.
* Some minor fixes (suggested by updated clippy).


## Release 0.11.2 (2022-07-01)

* Added a subcommand to "fetch" cover from local file.
* Updated from structopt to clap 3.2.
* Added this changelog!


## Release 0.11.0 (2022-05-14)
    
* Improve error handling.

  Don't (ab)use rejections for error handling.  Instead, have a specific
  type for any error that can be reported as a http response.

* Use tokio 1.0 with deadpool (PR #6).

  Rather than "faking" async diesel operations, use an async pool of
  plain synchronous database connections.  This makes it more obvious
  that we areactually blocking on the database operations.  Since the
  database operations tend to be fast and I use fewer connections than
  tokio executors, so there should be free executors for truly
  non-blocking operations.
    
* Over 1700 issues published!
* Use rust edition 2021.
* Update ructe to 0.14.0.
* Update env_logger to 0.9.
* Update scraper to 0.13.0.
* Minor cleanups, some suggest by clippy.


## Release 0.10.2 (2021-06-27)

* Fix a silly error in the migration
* Minor improvement in cloud weight distribution.

## Release 0.10.0 (2021-06-27)

* Minor cleanups (suggested by clippy)

* Improve creators overview

  Weight wringing or drawing comics rate higher than coloring or
  lettering when selecting for the creators "cloud" on the front page.

  Also re-implement `creator_contributions` using three underlying views
  so it is less complex and can be rematerialized in about 30 ms rather
  than 2 seconds.

* Update ructe and regex.
* Andreas Eriksson is the editor now.

## Release 0.9.4 (2021-03-19)

* Add a meta description to all pages.
* Css bugfix: I accidentally used a -moz- value only.  Added other
  compat versions.
* Provide width and height for sc logo.


## Release 0.9.2 (2021-03-18)

* Bättre scan av vinjettbilden.  Tack till Michael Gillvén!  Dessutom
  lite ändrat i hur dess storlek beräknas.
* Fix medal style, it should not overlap with the teaser.
* Improve server filter structure.
  - First match the url, then the method (so errors are 404 and not bad
    method), and only then clone the db pool (to avoid unnessesary work,
    even if it's only an Arc::clone).
  - This implies that the `PgPool` is now the last argument to handlers,
    rather than the first.
* Improve search ux; every input should have a label.
* Improve decorative styling.  Use images as background rather than as
  generated content when they are just decorative.
* Changed vignette image size.
* UX: Link structure on year summary.
* Show best episode in year summary.
* Always show last year in YearLinks.
* Improve link contrast.
* Some markup cleanup.
* Clippy: Improve tests for empty strings.
* Update roxmltree to 0.14.
* Make struct Part more usefull.
* Use github rather than travis for ci.


## Release 0.9.0 (2020-12-07)

* PR #5: Rather than having a long year view with all details, have a
  year summary view and a detail view for each issue.
  (The old all-details-for-a-year view is still around, but only when
  explicitly requested.)
* Bugfix: Part name might need escapeing.
* Improve yearlinks: Add links for each decade, compensate by having
  fewer year links around the showing year.
* Some changes to the front page. There is just over 1675 issues.
* Improve handling of prevpub.
* Improve handling of one-shots with orig name.
* Use the "wide" look down to smaller sizes.
* Make bind address configurable with a `--bind` parameter to the
  run-server command.
* Improve small-screen cover list.
* Uncrop cover medals.
* Fix small screen view of yearsummry.
* Improve titles, creators and refkeys listings: Count actual
  episodes, rather than parts / publications.  And add a note
  explaining that to the list page.
* Fix some `Ord`: Don't derive Ord when implementing a custom
  PartialOrd.  Instead, implement a custom Ord and use that in the
  PartialOrd implementation.
* Improve commadline error handling.
  - Use anyhow rather than failure for handling errors that should
    be reported to a commandline user.
* Fix conditionally required commandline arguments.
* Update ructe to 0.13.
* Update env_logger to 0.8.
* Some refactoring.


## Release 0.8.2 (2020-10-09)

* Fix: Don't clear pages/price for prevpub.
* Refactor how the database connection url is handled.  Also, change
  connection timeout from the default 30s to 500 ms, since this is a
  web server.
* Lower minimum idle number for db pool, and log pool creation on
  debug level.
* Update ructe to 0.12.0.
* Update roxmltree to 0.13.0.
* Update scraper to 0.12.0.
* Some refactoring and clippy fixes.


## Release 0.8.0 (2020-04-27)

* Put medals on the top three positions in best cover and best episode.
* Changed "drygt 1600" to "drygt 1650" issues total (latest is 1667)
* Get rid of a silly little dependency.
* Update roxmltree to 0.11.0.
* Update ructe to 0.11.4, Improve tempates and output with `@match`.
* Remove failure from request-handling parts of fanrs.


## Release 0.7.0 2020-04-13)

* Use tokio-disel for async db.
* Improve creators cloud with pre-calculated creator_contributions table.
* Refactor the creators list page.


## Release 0.6.0 (2020-02-04)

* Include articles and covers (number and range) in creators listing.
  - Sice the data query for that initially took ~ 20 s to execute,
    optimize the data in two ways: First add a column "magic" to the
    issues table for storing the previously calculated single integer
    issue descriptor, and then add a materialized view for the entire
    query.
  - Use the precalculated "magic" value to optimize some queries
    throughout the application.
* PR #2: Upgrade warp and requests for async
* PR #1: Switch to roxmltree for reading data.
* Adapt large tables to small screens.
  - Just hide the word "Antal" if the screens is narrow.
  - Phone-frienly issue links.
* Linkify urls in notes.
* Use logger rather than println.
* Improve error logging.
* Bugfix: Avoid creating dupliates in prevpub.
* Fix mailto in error template.



## Release 0.5.0 (2019-12-30)

* Subcommand updates and improved fetch-covers.
  - Refactor how subcommands get the database url argument (now that one
    subcommand should not take it).
  - Improve help texts.
  - Make sure there is no more than one cover for each issue.
  - Refactor the fetch-covers implementation.
  - Add --update-old and --no-op flags to fetch-covers, to update some
    of the oldest cover images and to only list what covers would be
    attempted.
  - Improve list-issues output.
  - Refactor read-files command args.
  - Add a count_pages utility command.
* Add a fallback image for unknown covers.
* Add simple issue links in notes.
* Refactor article handling.
  - Add a FullArticle struct, similar to FullEpisode, with an article and
    its references and creators.
* Improve episode encapsualtion.
* Make clippy happy re returning errors.
* A runner may consume the cli args.
* Some more cleanup of count-pages.
* Tell clippy some large enum variants are ok.
* Improve error-handling in count-pages.
* Update ructe to 0.9.0.
* Update xmltree to 0.9.0.
* Improve error handling in server.
  - Don't use unwrap in code that is executed from the server.
* Remove a useless usage of failure.
* Update dotenv to 0.15.0 and improve error handling for it.
* Rustfmt.


## Release 0.4.0 (2019-10-08)

* Get rid of time dependency.
* Rewrite handling of price: Get rid of bigdecimal dependency, price
  is just a fixed point number, that can be handled just fine as an
  i32 with some parsing and formatting.
* Get rid of some unnessary static lifetimes.
* Use explicit dyn for trait objects.
* Update ructe to 0.7.2 (and remove a workaround for an old rsass bug).
* Update scraper to 0.11.
* Update structopt to 0.3.
* Update dotenv.
* Remove redundant imports.


## Release 0.3.4 (2019-04-20)

* Handle some more old urls.
* Update serde usage.
* Minor template markup improvement.
* Improve error handling in readfiles.
* Avoid another false positive for cover images.
* Update scraper dependency.
* Exclude known-by-reprint-only in list-issues.


## Release 0.3.2 (2019-03-25)

* Bugfix: There was a field in the search where I didn't use a
  case-insensitive search in the search page.


## Release 0.3.0 (2019-03-24)

* Improve vignette sizing.
* Some layout fixes.
* Refactor covers by an artist.
* Refactor other contributions.
* Refactor cloud creation.
* Refactor routers.
  - Handle HEAD requests as well as GET.
  - More moduar code with some subrouters.
* Some more refactorization and clippy issues.


## Release 0.2.8 (2019-03-21)

* Fix another little style issue.


## Release 0.2.6 (2019-03-20)

* Fix a silly (but rather harmless) bug in the previous release.


## Release 0.2.4 (2019-03-20)

* Add a robots.txt and fix a touch icon handler.


## Release 0.2.2 (2019-03-19)

* Add some old-url redirects.
* Some special cases, some handling of old url formats.


## Release 0.2.0 (2019-03-17)
    
* First release for actual production deployment.


## Initial commit (2018-10-07)

Just parse some data and write out a bit of debug information about
it.
