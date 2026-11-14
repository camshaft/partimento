# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/camshaft/partimento/releases/tag/v0.1.0) - 2026-11-14

### Other

- Merge pull request #12 from camshaft/release-attach-notice
- Test each [paths] key and an entry that is neither a file nor a directory
- Refuse a symbolic link at any part of a configured path
- Format the path and link checks as rustfmt does
- Read only the tree: refuse paths that leave it and symbolic links
- Format the fixture tests as rustfmt does
- Run the fixtures through the library and the command
- Add the fixtures of the format rules
- Format the read of spec.toml as rustfmt does
- Format the order of findings as rustfmt does
- Add the partimento check command
- Add the library interface: check, check_default, check_cached, Report and Cache
- Add the solver backend trait
- Add the format rules
- Add the model of a set's RFCs, sections and requirement blocks
- Add the set's configuration, read from spec.toml
- Add the reading of a tree's file as TOML
- Add the lock file the CI's --locked builds read
- build where jq can't read the pull request list
- Add the partimento crate's manifest and library interface
- build the programs only on the release pull request's merge
- end the attach step with a notice where the tag is at another commit
- build the programs in read-only jobs and attach them in the release job
- publish only on the merge of the release pull request
- add the release workflow (release-plz, crates.io Trusted Publishing, static Linux programs)
- add the ci workflow
- Initial commit
