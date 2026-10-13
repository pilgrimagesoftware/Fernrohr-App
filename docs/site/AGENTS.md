# Documentation site rules

The published documentation is a Jekyll site. Edit it only under `docs/site`.

## Structure

- `latest/` holds the current pages, and `vX.Y/` holds release snapshots. The root
  `index.html` redirects to `latest/`.
- `_data/versions.yml` is the version manifest, in display order. For each
  version, `pages` lists every page it carries: version-relative, without the
  trailing slash, with `index` for the landing page. The version selector reads
  `pages` to open the same page in another version. `nav` is the sidebar, in
  reading order. A new page goes in both lists, or the selector and the sidebar
  won't find it.
- `_layouts/default.html` is the only layout. `_includes/` holds the version
  selector. `assets/` holds the stylesheet, fonts and icons.

## Pages

- Every Markdown page starts with front matter containing `title` and `version`
  (`latest`, or the snapshot's id). The permalink comes from the file path, so
  `latest/reference/keybindings.md` serves at `/latest/reference/keybindings/`.
- Use ATX headings with one `#` title per page, sentence-case headings and link
  labels, and fenced code blocks with a language (`sh`, `toml`, `yaml`, `json`).
- Write keys as `<kbd>⌘⇧P</kbd>`. Use the command title as the palette shows it,
  and its default binding from `app/src/command.rs`.
- Link internally with `{{ '/latest/<page>/' | relative_url }}`, never with a bare
  absolute path, so the links survive the Pages base path. Keep Liquid to those
  links. Logic belongs in layouts and includes.
- Describe what a user can see and do, including the keyboard route and the mouse
  route. Leave internals to `docs/architecture.md` in the meta repo.
- Keep examples short and realistic, and don't write contrived tutorials. Use a
  `> ` blockquote for a note, and `{: .warning}` on the line after it for a
  gotcha.
- This is a public repository. Examples use placeholders only: `my-cluster`,
  `bastion.example.com`, `10.0.0.1`, `my-project`. Never use real cluster or
  context names, hosts, IP addresses, bastions, cloud project IDs or credentials.
- End each guide with a line naming the capability specs it summarizes, from
  `openspec/specs/` in the meta repo. A spec change then points at the page to
  update.

## Theme

The tokens in `assets/css/site.css` come from the application, and its header
comment names the source of each. Restyle by changing a token, not individual
rules. The sources are:

- palette, surfaces, accent and status tones: gpui-component's default theme and
  `app/src/ui/style.rs`, adjusted to the 4.5:1 text and 3:1 accent and status
  contrast floor from the `visual-language` spec;
- spacing: `app/src/ui/space.rs`;
- fonts: `app/src/ui/typography.rs` and `app/assets/fonts/`. Adamina is used for
  headings and navigation, Manrope for body text, and the `MONO_FONT_ALTERNATES`
  stack in `app/src/ui/theme.rs` for code;
- icon: `images/fernrohr-icon.png`, shipped here at 256px.

## When docs change

- A new user-facing feature ships with its documentation in the same change.
- A bug fix that changes documented behavior updates the page that describes it,
  and so does a fix that shows a page to be wrong.
- A new command updates `latest/reference/keybindings.md`, and a new MCP tool
  updates `latest/reference/mcp-tools.md`, once those pages exist.

## Validate and publish

Install the Ruby pinned in `.ruby-version`, then run this from `docs/site`:

```sh
bundle install
bundle exec ruby _scripts/check.rb
```

It builds the site and fails on any build warning, a `versions.yml` entry with
no page, a page missing from `versions.yml`, or a dead internal link. CI runs the
same script on every pull request and before every deploy. `PUBLISHING.md` lists
what it checks. Don't add tests on page wording: the checks cover structure only.

Pushes to `master` publish the site, so documentation merged to `develop` goes
live with the next release. See `PUBLISHING.md` for the release snapshot steps.
Edit a `vX.Y/` snapshot only to correct that release's documentation.
