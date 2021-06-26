-- This file should undo anything in `up.sql`

drop materialized view creator_contributions;
drop view creator_sum_episodes;
drop view creator_sum_articles;
drop view creator_sum_covers;

create materialized view creator_contributions
  (id, name, slug, n_episodes, n_covers, n_articles, first_issue, latest_issue)
as select
  c.id, c.name, c.slug, count(distinct eb.episode_id), count(distinct cb.id), count(distinct ab.article_id), min(magic), max(magic)
from (
   creators c
   left outer join (((((
       creator_aliases ca
       left outer join (
           episodes_by eb
	       left outer join episode_parts ep ON ep.episode = eb.episode_id
       ) on eb.by_id = ca.id
   )
   left outer join articles_by ab on ab.by_id=ca.id
   )
   left outer join publications p ON (p.episode_part = ep.id or p.article_id=ab.article_id)
   )
   left outer join covers_by cb ON cb.by_id = ca.id
   )
   left outer join issues i ON (i.id = p.issue OR i.id = cb.issue_id)
   ) on ca.creator_id = c.id
)
group by c.id, c.name, c.slug
order by c.name;
