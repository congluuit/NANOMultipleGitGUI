import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "src-tauri", "src/ipc/bindings.ts"] },
  {
    files: ["**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      // macOS 12 and older ship WebKit without regex lookbehind (Safari 16.4+). A lookbehind
      // literal is a parse error there, so the whole bundle fails and the window stays blank.
      "no-restricted-syntax": [
        "error",
        {
          selector: "Literal[regex.pattern=/\\(\\?<[=!]/]",
          message:
            "Regex lookbehind breaks the app on macOS 12 (WebKit < 16.4). Capture the boundary in a group instead.",
        },
      ],
    },
  },
  {
    // Vendored shadcn/ui components export variants next to components by design.
    files: ["src/components/ui/**"],
    rules: { "react-refresh/only-export-components": "off" },
  },
);
