-- This file should undo anything in `up.sql`
drop materialized view creator_contributions;
alter table issues drop column magic;
drop index publications_episode;
drop index publications_article;
