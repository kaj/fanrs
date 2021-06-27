-- Your SQL goes here

drop materialized view creator_contributions;


create view creator_sum_episodes (id, n, n_hi, n_mid, earliest, latest)
as select c.id, cast(count(distinct eb.episode_id) as integer),
  cast(sum(case when eb.role in ('by', 'text', 'bild') then 1 else 0 end) as integer),
  cast(sum(case when eb.role in ('redax', 'ink', 'orig') then 1 else 0 end) as integer),
  min(magic), max(magic)
from
   creators c
   left outer join (((
       creator_aliases ca
       left outer join (
           episodes_by eb
           left outer join episode_parts ep ON ep.episode = eb.episode_id
         ) on eb.by_id = ca.id
       )
       left outer join publications p ON p.episode_part = ep.id
     )
     left outer join issues i ON i.id = p.issue
   ) on ca.creator_id = c.id
group by c.id
order by c.name;


create view creator_sum_articles (id, n, earliest, latest)
as select c.id, cast(count(distinct ab.article_id) as integer), min(magic), max(magic)
from
   creators c
   left outer join ((
       creator_aliases ca
       left outer join articles_by ab on ab.by_id = ca.id
       left outer join publications p ON p.article_id = ab.article_id
     )
     left outer join issues i ON i.id = p.issue
   ) on ca.creator_id = c.id
group by c.id, c.name, c.slug
order by c.name;


create view creator_sum_covers (id, n, earliest, latest)
as select c.id, cast(count(distinct cb.issue_id) as integer), min(magic), max(magic)
from
   creators c
   left outer join ((
       creator_aliases ca
       left outer join covers_by cb on cb.by_id = ca.id
     )
     left outer join issues i ON i.id = cb.issue_id
   ) on ca.creator_id = c.id
group by c.id, c.name, c.slug
order by c.name;


create materialized view creator_contributions
  (id, name, slug, score, n_episodes, n_covers, n_articles, first_issue, latest_issue)
as select
  cr.id, cr.name, cr.slug,
  cast(e.n + 9*e.n_hi + 5*e.n_mid + 6*a.n + round(90*sqrt(c.n)) as integer),
  e.n,
  c.n,
  a.n,
  least(e.earliest, c.earliest, a.earliest),
  greatest(e.latest, c.latest, a.latest)
from
  creators cr
  left join creator_sum_episodes e on e.id = cr.id
  left join creator_sum_articles a on a.id = cr.id
  left join creator_sum_covers c on c.id = cr.id
  order by cr.name;
