// ESLint flat config (ESLint 9). Minimal on purpose: typescript-eslint
// recommended + eslint:recommended. Svelte files are not linted here —
// `svelte-check` already type-checks them, so the residual gap is just
// stylistic. If we add `eslint-plugin-svelte` later, gate it on
// `svelte/svelte` parser config in its own config block.
import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import globals from 'globals';

export default [
	{
		// Lint only the SvelteKit frontend source. Rust assets, the
		// generated `.svelte-kit/`, build outputs and the Tauri target
		// dir are handled elsewhere or shouldn't be touched at all.
		ignores: [
			'.venv/**',
			'.svelte-kit/**',
			'.playwright-cli/**',
			'build/**',
			'node_modules/**',
			'output/**',
			'src-tauri/**',
			'scripts/**',
			'eslint.config.js',
			'svelte.config.js',
			'vite.config.ts',
			// Svelte files: covered by svelte-check until we wire
			// eslint-plugin-svelte into this config.
			'**/*.svelte'
		]
	},
	js.configs.recommended,
	...tseslint.configs.recommended,
	{
		files: ['src/**/*.{ts,js}'],
		languageOptions: {
			ecmaVersion: 2024,
			sourceType: 'module',
			globals: {
				...globals.browser,
				...globals.node
			}
		},
		rules: {
			// We deliberately use `any` in a few places for Tauri payload
			// shapes. Re-tighten once we generate types from the Rust side.
			'@typescript-eslint/no-explicit-any': 'off'
		}
	},
	{
		// Test files commonly redefine globals or use `any`.
		files: ['src/**/*.test.{ts,js}', 'src/**/*.spec.{ts,js}'],
		rules: {
			'@typescript-eslint/no-unused-expressions': 'off'
		}
	}
];
