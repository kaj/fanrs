create table articles (
  id serial primary key,
  title varchar(200) not null,
  subtitle varchar(500),
  note text
  -- refkeys
  -- creators
);

create table article_refkeys (
  id serial primary key,
  article_id integer not null references articles(id),
  refkey_id integer not null references refkeys(id)
);

create unique index article_refkeys_natural on article_refkeys(article_id, refkey_id);

alter table publications add column article_id integer references articles(id);
