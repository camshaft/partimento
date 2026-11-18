# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.1](https://github.com/camshaft/partimento/compare/v0.7.0...v0.7.1) - 2026-11-18

### Added

- check that the RFCs of a requirement's same_contract_version share one version

## [0.7.0](https://github.com/camshaft/partimento/compare/v0.6.1...v0.7.0) - 2026-11-16

### Added

- diff fails a change of a contract RFC's requirements that doesn't raise its contract_version ([#99](https://github.com/camshaft/partimento/pull/99))
- [**breaking**] mark the model's shared types `#[non_exhaustive]` and build them through constructors ([#97](https://github.com/camshaft/partimento/pull/97))
- plan the files a render writes and removes ([#96](https://github.com/camshaft/partimento/pull/96))

### Fixed

- show a composed `exactly_one`'s `<->` with only the parts a conflict names ([#93](https://github.com/camshaft/partimento/pull/93))

### Other

- diff finds one unraised contract version for each contract RFC, and none for a system RFC ([#102](https://github.com/camshaft/partimento/pull/102))
- diff counts a change for the RFC that holds the requirement, not its identifier's ([#101](https://github.com/camshaft/partimento/pull/101))
- make solves under one assumption per part linear in the parts ([#95](https://github.com/camshaft/partimento/pull/95))
- a required requirement tied to a forbidden one by a `logic` formula, beside the same tie as relations ([#94](https://github.com/camshaft/partimento/pull/94))
- Build `Section` and `Rfc` through constructors ([#92](https://github.com/camshaft/partimento/pull/92))

## [0.6.1](https://github.com/camshaft/partimento/compare/v0.6.0...v0.6.1) - 2026-11-15

### Added

- check each registry entry against the kind tables ([#88](https://github.com/camshaft/partimento/pull/88))

### Fixed

- compose `exactly_one` requirements in clauses linear in their parts ([#87](https://github.com/camshaft/partimento/pull/87))

### Other

- the registry check's skip of a field its own check reports, and two kind tables of one name ([#89](https://github.com/camshaft/partimento/pull/89))

## [0.6.0](https://github.com/camshaft/partimento/compare/v0.5.1...v0.6.0) - 2026-11-15

### Added

- check the kind tables of section files against the registry ([#85](https://github.com/camshaft/partimento/pull/85))

## [0.5.1](https://github.com/camshaft/partimento/compare/v0.5.0...v0.5.1) - 2026-11-15

### Fixed

- a head whose registry doesn't read fails the diff ([#83](https://github.com/camshaft/partimento/pull/83))

### Other

- a registry format that is a text, and a min that isn't a whole number ([#82](https://github.com/camshaft/partimento/pull/82))
- check a registry under a directory that is a symbolic link ([#81](https://github.com/camshaft/partimento/pull/81))

## [0.5.0](https://github.com/camshaft/partimento/compare/v0.4.2...v0.5.0) - 2026-11-15

### Added

- check the registry of identifier kinds that spec.toml locates ([#79](https://github.com/camshaft/partimento/pull/79))

## [0.4.2](https://github.com/camshaft/partimento/compare/v0.4.1...v0.4.2) - 2026-11-15

### Fixed

- a parameter with a source doesn't hold a bound ([#77](https://github.com/camshaft/partimento/pull/77))

## [0.4.1](https://github.com/camshaft/partimento/compare/v0.4.0...v0.4.1) - 2026-11-15

### Other

- Rename fixture 37's RFC 101 to Couriers, and reword a comment in src/checks.rs ([#76](https://github.com/camshaft/partimento/pull/76))
- Test a permissive requirement's relations in the cycle and polarity checks, and its conditions ([#75](https://github.com/camshaft/partimento/pull/75))
- a permissive requirement's relations and bounds still count in the other checks ([#73](https://github.com/camshaft/partimento/pull/73))
- Load each tree once in `partimento diff` ([#71](https://github.com/camshaft/partimento/pull/71))
- Test a non-number parameter value and the diff's version line through the binary ([#72](https://github.com/camshaft/partimento/pull/72))

## [0.4.0](https://github.com/camshaft/partimento/compare/v0.3.2...v0.4.0) - 2026-11-15

### Other

- Check RFC groups, contract versions, required fields and parameter types ([#69](https://github.com/camshaft/partimento/pull/69))

## [0.3.2](https://github.com/camshaft/partimento/compare/v0.3.1...v0.3.2) - 2026-11-14

### Other

- An RFC's state follows from its directory, and the index stores none ([#67](https://github.com/camshaft/partimento/pull/67))

## [0.3.1](https://github.com/camshaft/partimento/compare/v0.3.0...v0.3.1) - 2026-11-14

### Other

- make the socket-entry test's tree under the system's temporary directory ([#65](https://github.com/camshaft/partimento/pull/65))

## [0.3.0](https://github.com/camshaft/partimento/compare/v0.2.0...v0.3.0) - 2026-11-14

### Other

- Reword two messages, three doc comments and fixture 24's RFC 0100 ([#64](https://github.com/camshaft/partimento/pull/64))
- Write a formula set's JSON for library hosts with a stack on the heap ([#59](https://github.com/camshaft/partimento/pull/59))
- Test the exactly_one key of check --formulas ([#63](https://github.com/camshaft/partimento/pull/63))
- Test the empty exactly_one in empty_compositions ([#62](https://github.com/camshaft/partimento/pull/62))
- Test the names of an exactly_one call's parts ([#61](https://github.com/camshaft/partimento/pull/61))
- Encode exactly_one in clauses linear in its parts ([#55](https://github.com/camshaft/partimento/pull/55))

## [0.2.0](https://github.com/camshaft/partimento/compare/v0.1.0...v0.2.0) - 2026-11-14

### Other

- Describe check --formulas in the README ([#58](https://github.com/camshaft/partimento/pull/58))
- Test that the formula set's JSON writes a formula of 100,000 nested `!` ([#57](https://github.com/camshaft/partimento/pull/57))
- Compare and debug-print logic formulas without recursion ([#56](https://github.com/camshaft/partimento/pull/56))
- Keep a conflicting consistency result in the cache ([#54](https://github.com/camshaft/partimento/pull/54))
- Add check --formulas and a timing test of 2,500 generated requirements ([#50](https://github.com/camshaft/partimento/pull/50))
- Hold the toml crate's loads of fixed probes in each section file's key ([#51](https://github.com/camshaft/partimento/pull/51))
- Test deep at_least_one calls and long implication chains ([#52](https://github.com/camshaft/partimento/pull/52))
- pin the parser's results for chains, n-ary groups and errors in calls ([#53](https://github.com/camshaft/partimento/pull/53))
- Test the render artifact's fallback and keeps() gate over a tree that renders ([#48](https://github.com/camshaft/partimento/pull/48))
- Parse and walk logic formulas without recursion ([#46](https://github.com/camshaft/partimento/pull/46))
- Hold Cargo.toml in the build identity ([#47](https://github.com/camshaft/partimento/pull/47))
- Keep the solver's and the render's artifacts in the cache ([#45](https://github.com/camshaft/partimento/pull/45))

## [0.1.0](https://github.com/camshaft/partimento/compare/v0.0.0...v0.1.0) - 2026-11-14

### Other

- Hold the build's code in a section file's cache key ([#41](https://github.com/camshaft/partimento/pull/41))
- four fixtures for choices of checks 1 to 5 that no test asserted ([#44](https://github.com/camshaft/partimento/pull/44))
- walk back from each forbidden requirement in the polarity check ([#43](https://github.com/camshaft/partimento/pull/43))
- Write the README: install, the specification tree, check, render, diff and the library ([#42](https://github.com/camshaft/partimento/pull/42))
- Post a relation finding for each `supersedes` cycle ([#40](https://github.com/camshaft/partimento/pull/40))
- Test that drift is checked where a check fails ([#39](https://github.com/camshaft/partimento/pull/39))
- One test of a generated path, shared by the drift rule and the render command ([#33](https://github.com/camshaft/partimento/pull/33))
- a relation to a contract RFC's requirement passes the system boundary check ([#38](https://github.com/camshaft/partimento/pull/38))
- a superseded requirement's resolve and boundary findings leave the report ([#37](https://github.com/camshaft/partimento/pull/37))
- Refuse a section anchor or title that the rendered heading can't hold ([#32](https://github.com/camshaft/partimento/pull/32))
- Skip the key, encoding and seal where the cache doesn't keep artifacts ([#36](https://github.com/camshaft/partimento/pull/36))
- Add the solver backends, the logic module and checks 1 to 5 ([#34](https://github.com/camshaft/partimento/pull/34))
- Say when a change to a section's load raises ENCODING
- render under the generated paths spec.toml sets, and a file render can't write ([#31](https://github.com/camshaft/partimento/pull/31))
- Format the cached check's call as rustfmt does
- Read the digest's words through as_chunks
- Add --cache <dir> to the check command, kept on disk
- Load each section file through the check's cache
- Read and write each section file's load as a cached artifact
- Test that a cached check reports what an uncached check reports
- Add the versioned, sealed artifact encoding of a section file's load
- Add a SHA-256 digest for cache keys and artifact seals
- pin each action by its full commit id
- Render the requirement set and check the generated files for drift ([#24](https://github.com/camshaft/partimento/pull/24))
- run the crates.io auth only where plan asks for a release ([#25](https://github.com/camshaft/partimento/pull/25))
- test a change of each compared field alone ([#22](https://github.com/camshaft/partimento/pull/22))
- Add diff: the requirements added, changed or retired between two trees ([#19](https://github.com/camshaft/partimento/pull/19))
- Format the embed macro of tests/configuration.rs as rustfmt does
- Test a fixture whose spec.toml changes each value
- Fail the unreadable-file test in CI where the file stays readable
- build the commit release-plz tags after a merge commit ([#14](https://github.com/camshaft/partimento/pull/14))
- Test the identifier, field and spec.toml rules and the command's output
- Merge pull request #12 from camshaft/release-attach-notice
- Test the line of the UTF-8 and parse findings
- Test each [paths] key and an entry that is neither a file nor a directory
- Read each file's newlines once to answer the line of a span
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
- build the commit release-plz tags after a merge commit
- build where jq can't read the pull request list
- Add the partimento crate's manifest and library interface
- build the programs only on the release pull request's merge
- end the attach step with a notice where the tag is at another commit
- build the programs in read-only jobs and attach them in the release job
- publish only on the merge of the release pull request
- add the release workflow (release-plz, crates.io Trusted Publishing, static Linux programs)
- add the ci workflow
- Initial commit
