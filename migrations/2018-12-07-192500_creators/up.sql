-- Your SQL goes here

create table creators (
  id serial primary key,
  name varchar(200) unique not null,
  slug varchar(200) unique not null
);

-- create unique index creators_name on creators(name);
-- create unique index creators_slug on creators(slug);

create table creator_aliases (
  id serial primary key,
  creator_id integer not null references creators(id),
  name varchar(200) unique not null
);

create table covers_by (
  id serial primary key,
  issue_id integer not null references issues(id),
  by_id integer not null references creator_aliases(id)
  -- Should there be a role here as well?  ink / pencils / etc?
);

create unique index covers_by_natural on covers_by(issue_id, by_id);

create table episodes_by (
  id serial primary key,
  episode_id integer not null references episodes(id),
  by_id integer not null references creator_aliases(id),
  role varchar(10) not null
);

create unique index episodes_by_natural on episodes_by(episode_id, by_id, role);

create table articles_by (
  id serial primary key,
  article_id integer not null references articles(id),
  by_id integer not null references creator_aliases(id),
  role varchar(10) not null
);

create unique index articles_by_natural on articles_by(article_id, by_id, role);
