# Checklist for Cutting a Release

Before cutting a release, please check the following tasks.

## Pre-release

- [ ] If there are changes to release pipeline in `.github/workflows/release.yml`,
      please create a pre-release to test such changes.

## Documentation

- [ ] Document new features in this book.
- [ ] Update this book if some features are changed 
- [ ] Update `CHANGELOG.md` to document 
  - notable changes compared to previous stable release for a stable release or release candidate,
  - or notable changes compared to previous unstable release for an unstable `alpha/beta` release.
- [ ] If any CLI flags are changed/added, ensure that `README.md` is up-to-date by running `just update-readme`.

## Chores

- [ ] Bump version with `just bump <level>`, where `<level>` is `major`, `minor` or `patch`.
- [ ] Ensure lockfile is updated after previous step.
- [ ] Commit the changes with the following commit message template: `release: <VERSION>`.
- [ ] Create a signed git tag named `v<VERSION>`.
- [ ] Push the commit and git tag to remote.
- [ ] After the release pipeline successfully finishes, edit the release in GitHub Releases to publish the draft release.
