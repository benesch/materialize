# Project management

A **project** is a bundle of related engineering work that spans one or more
weeks.

The term "project" replaces our previous use of the term "epic."

Small projects last one or two weeks, while large projects can span multiple
quarters.

Large projects require more planning. They **must** be broken into smaller
milestones. Each milestone should represent a "pause point", where the project
can be set down without incurring undue context switching cost.

Every project has a project manager, a primary engineer, and a tracking issue.

## Responsibilities

### Project manager

The project manager is primarily responsible for *communication*.

### Engineering lead

## Tracking issue

See the issue template.

## Status

* Ideating
* Prototyping
* Alpha
* Beta
* Stable
* Paused
* Abandoned

## Non-negotiables

### Design doc

* Any change with cross-team dependencies **must** have a design document.
* Any change to Materialize's public APIs **must** have a design document.
  Public APIs include:

  * The SQL parser.
  * The system catalog.
  * The cloud global API.
  * The cloud region API.
  * The `mz` CLI.

  Note that the web UI is *not* a public API, as it is consumed by humans,
  not machines.


## Negotiables

* Have you collaborated with the docs team to write guides and DevEx features?
