// Minimal lint config: tsc (strict, noUnusedLocals) is the main static check;
// ESLint exists to enforce what tsc can't see — React hooks rules.
import tseslint from 'typescript-eslint'
import reactHooks from 'eslint-plugin-react-hooks'

export default tseslint.config(
  { ignores: ['dist', 'src-tauri', 'node_modules', 'bin'] },
  {
    files: ['src/**/*.{ts,tsx}'],
    languageOptions: { parser: tseslint.parser },
    plugins: { 'react-hooks': reactHooks },
    // Just the classic hook rules; the v7 "recommended" preset adds the React
    // Compiler lints (refs/immutability/purity), which this codebase predates.
    rules: {
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',
    },
  },
)
