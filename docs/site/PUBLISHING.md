# Publishing

`.github/workflows/docs-pages.yml` builds this site and deploys it to GitHub
Pages. The repository's Pages source must be set to **GitHub Actions** (Settings
-> Pages -> Build and deployment -> Source). The workflow runs on pushes to
`master` that change `docs/site/**` or the workflow itself, and it can also be
started by hand (`workflow_dispatch`). Pull requests that change the site run the
build without deploying, and pushes to `develop` never publish. A failed
build stops before the deploy job, and the live site stays as it was.

## Before merging

The workflow checks every pull request that touches the site, and every deploy
runs the same checks first, so a failure blocks the deploy. To run them locally,
from `docs/site`:

```sh
bundle install
bundle exec ruby _scripts/check.rb
```

The script:

1. Builds the site with an empty base URL. Any build warning fails the check, and
   so does an unknown Liquid filter (`strict_filters` in `_config.yml`).
2. Checks `_data/versions.yml` (`_scripts/check-manifest.rb`). Every `pages`
   entry needs a source file and a built page, every `nav` path needs to be in its
   version's `pages`, every page under a version directory needs to be listed,
   and every version directory needs to be in the manifest.
3. Checks every internal link and anchor in `_site` with html-proofer. External
   links aren't checked.

The checks look at structure only, never at wording.

## Cutting a release snapshot

Cut the snapshot for a release `vX.Y.0` on `develop` before running the Prepare
Release workflow. The release PR then carries it to `master`, and the release
deploy publishes it. Snapshotting after the tag would need a second merge to
`master` to publish it.

1. Copy `latest/` to `vX.Y/`, and change every `version: latest` in it to
   `version: vX.Y`. Rewrite its `/latest/` links to `/vX.Y/`.
2. In `vX.Y/index.md`, name the release in the title and heading
   (`Fernrohr vX.Y documentation`), and replace "the latest release" with
   "the Fernrohr vX.Y release".
3. Add a `vX.Y` entry to `_data/versions.yml`, after `latest` and before older
   releases, with the same `pages` and `nav` as `latest`.
4. Run `bundle exec ruby _scripts/check.rb`, then switch between `latest` and
   `vX.Y` on a few pages.

If Prepare Release picks a different version than the snapshot assumed, rename
the directory, its `version:` values and links, and the manifest entry before the
release PR merges.

## After a deploy

Open the root URL, `/latest/`, and each `/vX.Y/` in the manifest. Switch
versions from a page that exists in both, and from one that exists only in
`latest`.
