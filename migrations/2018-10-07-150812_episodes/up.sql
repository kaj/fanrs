-- Create the episodes table

create table episodes (
  id serial primary key,
  title integer not null references titles (id),
  episode varchar,
  teaser varchar,
  note varchar,
  copyright varchar
);

create unique index episodes_natural on episodes (title, episode);
