import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "src-tauri", "node_modules"] },
  js.configs.recommended,
  ...tseslint.configs.strictTypeChecked.map((cfg) => ({
    ...cfg,
    files: ["**/*.ts"],
  })),
  {
    files: ["**/*.ts"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/consistent-type-imports": "error",
      // Numbers in messages and CSS strings are intentional.
      "@typescript-eslint/restrict-template-expressions": ["error", { allowNumber: true }],
      // `invoke<void>` is the idiomatic return type for commands that return nothing.
      "@typescript-eslint/no-invalid-void-type": ["error", { allowInGenericTypeArguments: true }],
      // DOM handlers are often written as expression-bodied arrows that call void functions.
      "@typescript-eslint/no-confusing-void-expression": "off",
    },
  },
);
