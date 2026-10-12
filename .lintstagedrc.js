// Staged files only; clippy and tests run in CI to keep commits fast.
export default {
  "*.{ts,tsx}": ["prettier --write", "eslint --fix", () => "tsc --noEmit"],
  "*.{js,json,css,md,html}": ["prettier --write"],
  "src-tauri/**/*.rs": ["rustfmt --edition 2021"],
};
