import js from '@eslint/js'
import globals from 'globals'
import tseslint from 'typescript-eslint'
import reactHooks from 'eslint-plugin-react-hooks'

export default tseslint.config(
  { ignores: ['dist'] },
  js.configs.recommended,
  tseslint.configs.recommended,
  // The `flat` variant; the top-level one is still in the legacy shape.
  reactHooks.configs.flat['recommended-latest'],
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
  },
  {
    // The config itself and the build-time tools run under Node, not in a
    // browser: `process`, `console` and `URL` are theirs.
    files: ['*.config.{js,ts}', 'tools/**/*.mjs'],
    languageOptions: { globals: globals.node },
  },
)
