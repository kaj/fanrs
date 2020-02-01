-- Your SQL goes here
alter table issues add column magic smallint;
update issues set magic = cast(((year-1950)*64+number)*2 + sign(cast(position('-' in number_str) as smallint)) as smallint);

alter table issues alter column magic set not null;

create unique index issue_magic on issues (magic);
create index publications_article on publications(article_id);
create index publications_episode on publications(episode_part);

CREATE MATERIALIZED VIEW creator_contributions (id, name, slug, n_episodes, n_covers, n_articles, first_issue, latest_issue)
AS SELECT c.id, c.name, c.slug, count(distinct eb.episode_id), count(distinct cb.id), count(distinct ab.article_id), min(magic), max(magic)
FROM (
   creators c
   LEFT OUTER JOIN (((((
       creator_aliases ca
       LEFT OUTER JOIN (
           episodes_by eb
	       LEFT OUTER JOIN episode_parts ep ON ep.episode = eb.episode_id
       ) ON eb.by_id = ca.id
   )
   LEFT OUTER JOIN articles_by ab on ab.by_id=ca.id
   )
   LEFT OUTER JOIN publications p ON (p.episode_part = ep.id or p.article_id=ab.article_id)
   )
   LEFT OUTER JOIN covers_by cb ON cb.by_id = ca.id
   )
   LEFT OUTER JOIN issues i ON (i.id = p.issue OR i.id = cb.issue_id)
   ) ON ca.creator_id = c.id
)
GROUP BY c.id, c.name, c.slug
ORDER BY c.name;
