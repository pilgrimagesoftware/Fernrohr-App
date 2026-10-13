# Publishing

`.github/workflows/docs-pages.yml` builds this site and deploys it to GitHub
Pages. The repository's Pages source must be set to **GitHub Actions** (Settings
-> Pages -> Build and deployment -> Source). The workflow runs on pushes to
`master` that change `docs/site/**` or the workflow itself, and it can also be
started by hand (`workflow_dispatch`). Pull requests that change the site run the
build without deploying, and pushes to `develop` never publish. A failed
build stops before the deploy job, and the live site stays as it was.

## Before merging

From `docs/site`, run `bundle install` and `bundle exec jekyll build`. Confirm
that `_site/index.html` redirects to `latest/` and that every path in
`_data/versions.yml` exists under `_site/`.

## Cutting a release snapshot

When a release `vX.Y.0` is tagged:

1. Copy `latest/` to `vX.Y/`, and change every `version: latest` in it to
   `version: vX.Y`. Rewrite its `/latest/` links to `/vX.Y/`.
2. Add a `vX.Y` entry to `_data/versions.yml`, after `latest` and before older
   releases, with the same `pages` and `nav` as `latest`.
3. Build locally and switch between `latest` and `vX.Y` on a few pages.

## After a deploy

Open the root URL, `/latest/`, and each `/vX.Y/` in the manifest. Switch
versions from a page that exists in both, and from one that exists only in
`latest`.
