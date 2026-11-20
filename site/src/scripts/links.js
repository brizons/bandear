// The repository's GitHub addresses, in one place for the pages and the scripts. Every download
// link in the built pages opens RELEASES_PAGE until downloads.js finds the files;
// tests/downloads.test.mjs checks the built pages. If the repository moves, change them here.

export const REPOSITORY = "https://github.com/brizons/bandear";
export const RELEASES_PAGE = `${REPOSITORY}/releases/latest`;
export const RELEASE_API = "https://api.github.com/repos/brizons/bandear/releases/latest";
export const LICENSE_PAGE = `${REPOSITORY}/blob/main/LICENSE`;

// The release the built pages name until downloads.js reads the latest one (the version chip and
// the file names on the download cards). Bump it with each release.
export const FALLBACK_VERSION = "0.4.2";
