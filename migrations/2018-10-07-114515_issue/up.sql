-- Create the issue table

create table issues (
  id serial primary key,
  year smallint not null,
  number smallint not null,
  number_str varchar(6) not null,
  pages smallint,
  price decimal(5, 2),
  cover_best smallint
);

create unique index issue_natural on issues (year, number);
