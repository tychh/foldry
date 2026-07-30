# Feedback and contribution policy

Foldry is currently developed and maintained as a single-author project.

Bug reports, usability feedback, and feature requests are welcome. Code
contributions, patches, and pull requests are not being accepted at this time.
Please do not invest time in an implementation with the expectation that it will
be merged upstream.

This policy keeps the product direction and maintenance workload manageable. It
is not a judgment on the quality of outside contributions, and it may change in
the future.

## Report a bug

Before opening an issue:

1. Check that the problem still occurs in the latest release.
2. Search existing issues for the same behavior.
3. Remove private paths, credentials, and personal data from logs and screenshots.

A useful bug report includes:

- Foldry version and installation type;
- operating system, architecture, and filesystem;
- a short sequence that reproduces the problem;
- expected and actual behavior;
- archive format and relevant action settings;
- sanitized logs, screenshots, or a disposable test tree when helpful.

Keep one independently actionable problem per issue.

## Suggest a feature

Feature requests should explain the problem or workflow first:

- what you are trying to accomplish;
- why the current workflow is insufficient;
- whether the request concerns the desktop application, CLI, or both;
- important safety, privacy, or cross-platform constraints;
- what a successful outcome would look like.

Mockups and examples are welcome. A proposed implementation is not required.
Requests may be declined or deferred when they do not fit the current product
direction or maintenance capacity.

## Security reports

Do not report a vulnerability in a public issue. Follow the
[security policy](SECURITY.md) and use a private GitHub Security Advisory.

## Pull requests and forks

Unsolicited pull requests may be closed without code review. Please open an issue
when you want to report a bug or discuss a feature.

The project license permits forks and independent modifications. The
[development documentation](docs/development/README.md) remains available for
people studying, building, or adapting the code, but upstream integration and
support for fork-specific changes are not implied.
