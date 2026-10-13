# frozen_string_literal: true

# Validates the site the way CI does: builds it with an empty base URL, fails
# on any build warning, then checks the version manifest and every internal
# link in the output. External links are not checked.
#
#   bundle exec ruby _scripts/check.rb
#
# Run from `docs/site`. It leaves the checked build in `_site`.

require "open3"
require "html-proofer"

# The pages are UTF-8. Under a C or POSIX locale Ruby reads them as US-ASCII, and
# html-proofer then skips every page it fails to parse and still reports success.
Encoding.default_external = Encoding::UTF_8

Dir.chdir(File.expand_path("..", __dir__))

puts "Building the site..."
output, status = Open3.capture2e("jekyll", "build", "--baseurl", "")
warnings = output.lines.grep(/warn|error|deprecat/i)
unless status.success? && warnings.empty?
  warn output
  abort "Build failed or printed warnings."
end

system(Gem.ruby, "_scripts/check-manifest.rb") || abort

HTMLProofer.check_directory(
  "_site",
  disable_external: true,
  checks: %w[Links Images Scripts],
  check_internal_hash: true,
  enforce_https: false,
).run
