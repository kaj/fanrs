create table refkeys (
  id serial primary key,
  kind smallint not null,
  title varchar(100),
  slug varchar(100) not null
);

create unique index refkey_ks on refkeys(kind, slug);

create table episode_refkeys (
  id serial primary key,
  episode_id integer not null references episodes(id),
  refkey_id integer not null references refkeys(id)
);

create unique index episode_refkeys_natural on episode_refkeys(episode_id, refkey_id);
