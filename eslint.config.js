import eslintPluginVue from 'eslint-plugin-vue';
import typescriptEslint from 'typescript-eslint';
import { withVueTs, vueTsConfigs } from '@vue/eslint-config-typescript';
import stylistic from '@stylistic/eslint-plugin';

export default withVueTs(
    {
        ignores: [
            '**/node_modules/**',
            '**/dist/**',
            '**/src-tauri/**',
        ],
    },

    ...typescriptEslint.configs.recommended,
    ...eslintPluginVue.configs['flat/recommended'],
    vueTsConfigs.recommended,

    {
        files: ['src/**/*.{ts,vue}'],
        languageOptions: {
            parserOptions: {
                projectService: true,
                extraFileExtensions: ['.vue'],
            },
        },
    },

    {
        files: ['src/**/*.{ts,vue}'],
        plugins: {
            '@stylistic': stylistic,
        },
        rules: {
            '@stylistic/quotes': ['error', 'single', { avoidEscape: true }],
            '@stylistic/semi': ['error', 'always'],
            '@stylistic/comma-dangle': ['error', 'always-multiline'],
            'vue/html-quotes': ['error', 'double'],
            'vue/multi-word-component-names': 'off',
        },
    }
);
