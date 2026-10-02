
## 0.2.0 - 2026-10-02

### <!-- 0 -->🚀 Features
- Add Service, Ingress, Endpoints and NetworkPolicy sections
- Add PersistentVolume and StorageClass sections
- Add CronJob and Namespace sections
- Add Role, ClusterRole and binding sections
- Watch any discovered kind into a shared object table
- Add ObjectListPanel, a live list of any discovered kind
- Open every non-Pod kind as a list panel
- Open a list row's object, focusing it if already open
- Save and restore list panels with their column layout
- Add the per-kind column model and cell sorting
- Add workload list columns
- Add config and network list columns
- Add storage, cluster and access-control list columns
- Open the Resource panel on a stored preferred edge
- Ring the focused panel in the accent colour
- Title custom resource lists by kind, group in the tab tooltip
- Describe/YAML keys and first-row Up/Down on list panels
- Close the panel each close control belongs to, the last one too
- Stay connected when the last panel closes; close beside the title
- Fit table columns to their contents; widen the Pods Name column


### <!-- 1 -->🐛 Bug Fixes
- Grant id-token:write so the release workflow can start
- Port report_issue dialog to gpui-kit 0.7's WindowExt API
- Skip fontconfig pkg-config lookup during release Test step
- Let the Resource panel move, collapse and show its focus
- Keep focus on a panel when the focused tab closes
- Focus the displayed panel when a window enters its workspace
- Focus a panel when its displayed tab is clicked
- Drop the panel focus ring; the tab title marks focus
- Keep the Resource panel's header focus bar
- Save a nested split's sizes as drawn, not as first created
- Bring the app and its first window to the front at launch
- Keep Tab inside the focused panel
- Every close of the focused panel leaves a panel focused
- Open new panels in the tab group that last had focus
- Keep a tab strip on lone groups so tabs can be dropped on them


### <!-- 2 -->🚜 Refactor
- Share the watch stream and key session watches by kind
- Drop the list watch's UNWIRED markers


### <!-- 3 -->📚 Documentation
- Name the real pause-info caller in watch_registry


### <!-- 6 -->🧪 Testing
- Drop redundant closures clippy flags
- Check every listed kind gets sections
- Pass register_handler to cx.update directly
- Build the nested split by moving panels, not split_at


### <!-- 7 -->⚙️ Miscellaneous Tasks
- Report issue dialog


# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Repository scaffolding with CI, testing, and contribution guidelines
- Rust project structure with GPUI and GPUI-Kit dependencies
- Initial GitHub Actions workflows for build and lint checks
