
drop trigger episode_ts_update on episodes;
drop trigger article_ts_update on articles;

drop function episodes_trigger;
drop function articles_trigger;
drop function make_tsv;

alter table episodes drop column ts;
alter table articles drop column ts;
