
alter table episodes add column ts tsvector;
alter table articles add column ts tsvector;

create function make_tsv(s text) returns tsvector as $$
   select to_tsvector(replace(coalesce(s, ''), U&'\00AD', ''))
$$ language sql;

update episodes set ts =
   setweight(make_tsv(episode), 'A') ||
   setweight(make_tsv(teaser), 'B') ||
   setweight(make_tsv(note), 'C') ||
   setweight(make_tsv(copyright), 'D');

create function episodes_trigger() returns trigger as $$
begin
  new.ts :=
   setweight(make_tsv(new.episode), 'A') ||
   setweight(make_tsv(new.teaser), 'B') ||
   setweight(make_tsv(new.note), 'C') ||
   setweight(make_tsv(new.copyright), 'D');
  return new;
end
$$ language plpgsql;


update articles set ts =
   setweight(make_tsv(title), 'A') ||
   setweight(make_tsv(subtitle), 'A') ||
   setweight(make_tsv(note), 'B');

create function articles_trigger() returns trigger as $$
begin
  new.ts :=
   setweight(make_tsv(new.title), 'A') ||
   setweight(make_tsv(new.subtitle), 'A') ||
   setweight(make_tsv(new.note), 'B');
  return new;
end
$$ language plpgsql;

create trigger episode_ts_update before insert or update
on episodes for each row execute function episodes_trigger();

create trigger article_ts_update before insert or update
on articles for each row execute function articles_trigger();

alter table episodes alter column ts set not null;
alter table articles alter column ts set not null;
