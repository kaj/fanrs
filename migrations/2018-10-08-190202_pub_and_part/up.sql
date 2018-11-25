-- Your SQL goes here
create table episode_parts (
  id serial primary key,
  episode integer not null references episodes(id),
  part_no smallint,
  part_name varchar(200)
);
create unique index episode_parts_natural on episode_parts(episode, coalesce(part_no, 0), coalesce(part_name, ''));

create table publications (
  id serial primary key,
  issue integer not null references issues(id),
  seqno smallint,
  episode_part integer references episode_parts(id),
  best_plac smallint
  -- todo: article
);
create unique index publications_natural on publications(issue, episode_part);
