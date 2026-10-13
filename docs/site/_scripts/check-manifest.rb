# frozen_string_literal: true

# Checks `_data/versions.yml` against the source tree and the built `_site`:
#
# - every `pages` entry has a source page and a built page;
# - every `nav` path is one of its version's `pages`;
# - every page under a version directory is listed in that version's `pages`;
# - every version directory (`latest/`, `vX.Y/`) is in the manifest.
#
# Run from `docs/site` after a build. It exits non-zero and lists every
# problem it finds. Standard library only.

require "yaml"

SITE_ROOT = File.expand_path("..", __dir__)
BUILT = File.join(SITE_ROOT, "_site")
PAGE_EXTENSIONS = %w[.md .markdown .html].freeze
VERSION_DIR = /\A(latest|v\d+\.\d+)\z/

# A page id is version-relative with no extension: `index`, `reference/keybindings`.
def page_id(path)
  path.delete_suffix("/").then { |p| p.empty? ? "index" : p }
end

def source_page?(dir, id)
  PAGE_EXTENSIONS.any? { |ext| File.file?(File.join(SITE_ROOT, dir, id + ext)) }
end

def built_page?(dir, id)
  built = id == "index" ? File.join(dir, "index.html") : File.join(dir, id, "index.html")
  File.file?(File.join(BUILT, built))
end

def source_pages(dir)
  Dir.glob("**/*{#{PAGE_EXTENSIONS.join(',')}}", base: File.join(SITE_ROOT, dir))
     .map { |file| file.sub(/\.[^.\/]+\z/, "") }
end

problems = []
abort "No built site at #{BUILT}: run `bundle exec jekyll build` first." unless Dir.exist?(BUILT)

manifest = YAML.safe_load_file(File.join(SITE_ROOT, "_data", "versions.yml"))
versions = manifest.fetch("versions")
listed_dirs = versions.map { |v| v.fetch("path").delete_prefix("/").delete_suffix("/") }

versions.each do |version|
  id = version.fetch("id")
  dir = version.fetch("path").delete_prefix("/").delete_suffix("/")
  pages = version.fetch("pages")

  pages.each do |page|
    problems << "#{id}: page `#{page}` has no source file in #{dir}/" unless source_page?(dir, page)
    problems << "#{id}: page `#{page}` was not built" unless built_page?(dir, page)
  end

  version.fetch("nav").each do |item|
    target = page_id(item.fetch("path"))
    problems << "#{id}: nav entry `#{item['title']}` points at `#{target}`, which is not in pages" unless pages.include?(target)
  end

  (source_pages(dir) - pages).each do |page|
    problems << "#{id}: #{dir}/#{page} is not listed in pages"
  end
end

Dir.children(SITE_ROOT).grep(VERSION_DIR).each do |dir|
  next unless File.directory?(File.join(SITE_ROOT, dir))

  problems << "#{dir}/ is not a version in _data/versions.yml" unless listed_dirs.include?(dir)
end

if problems.empty?
  puts "Manifest OK: #{versions.size} versions, #{versions.sum { |v| v['pages'].size }} pages."
else
  warn "Manifest check failed:"
  problems.each { |problem| warn "  - #{problem}" }
  exit 1
end
