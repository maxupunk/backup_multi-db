/**
 * plugins/vuetify.ts
 *
 * Framework documentation: https://vuetifyjs.com
 */

// Composables
import { createVuetify, type ThemeDefinition } from 'vuetify'
// Styles
import '@mdi/font/css/materialdesignicons.css'

import 'vuetify/styles'

// Tema escuro customizado
const darkTheme: ThemeDefinition = {
  dark: true,
  colors: {
    'background': '#0a0e17',
    'surface': '#111827',
    'surface-bright': '#1f2937',
    'surface-light': '#374151',
    'surface-variant': '#1e293b',
    'on-surface-variant': '#94a3b8',
    'primary': '#3b82f6',
    'primary-darken-1': '#2563eb',
    'secondary': '#8b5cf6',
    'secondary-darken-1': '#7c3aed',
    'error': '#ef4444',
    'info': '#06b6d4',
    'success': '#22c55e',
    'warning': '#f59e0b',
    'on-background': '#f1f5f9',
    'on-surface': '#e2e8f0',
    'on-primary': '#ffffff',
    'on-secondary': '#ffffff',
    'on-error': '#ffffff',
    'on-info': '#ffffff',
    'on-success': '#ffffff',
    'on-warning': '#000000',
  },
  variables: {
    'border-color': '#1e293b',
    'border-opacity': 0.12,
    'high-emphasis-opacity': 0.87,
    'medium-emphasis-opacity': 0.6,
    'disabled-opacity': 0.38,
    'idle-opacity': 0.04,
    'hover-opacity': 0.08,
    'focus-opacity': 0.12,
    'selected-opacity': 0.08,
    'activated-opacity': 0.12,
    'pressed-opacity': 0.12,
    'dragged-opacity': 0.08,
    'theme-kbd': '#1f2937',
    'theme-on-kbd': '#e2e8f0',
    'theme-code': '#1e293b',
    'theme-on-code': '#e2e8f0',
  },
}

// Tema claro customizado
const lightTheme: ThemeDefinition = {
  dark: false,
  colors: {
    'background': '#f8fafc',
    'surface': '#ffffff',
    'surface-bright': '#ffffff',
    'surface-light': '#f1f5f9',
    'surface-variant': '#e2e8f0',
    'on-surface-variant': '#64748b',
    'primary': '#2563eb',
    'primary-darken-1': '#1d4ed8',
    'secondary': '#7c3aed',
    'secondary-darken-1': '#6d28d9',
    'error': '#dc2626',
    'info': '#0891b2',
    'success': '#16a34a',
    'warning': '#d97706',
    'on-background': '#0f172a',
    'on-surface': '#1e293b',
    'on-primary': '#ffffff',
    'on-secondary': '#ffffff',
    'on-error': '#ffffff',
    'on-info': '#ffffff',
    'on-success': '#ffffff',
    'on-warning': '#000000',
  },
  variables: {
    'border-color': '#e2e8f0',
    'border-opacity': 0.12,
    'high-emphasis-opacity': 0.87,
    'medium-emphasis-opacity': 0.6,
    'disabled-opacity': 0.38,
    'idle-opacity': 0.1,
    'hover-opacity': 0.04,
    'focus-opacity': 0.12,
    'selected-opacity': 0.08,
    'activated-opacity': 0.12,
    'pressed-opacity': 0.12,
    'dragged-opacity': 0.08,
    'theme-kbd': '#f1f5f9',
    'theme-on-kbd': '#1e293b',
    'theme-code': '#f1f5f9',
    'theme-on-code': '#1e293b',
  },
}

export default createVuetify({
  theme: {
    defaultTheme: 'dark',
    themes: {
      dark: darkTheme,
      light: lightTheme,
    },
  },
  defaults: {
    VCard: {
      rounded: 'lg',
      elevation: 0,
    },
    VBtn: {
      rounded: 'lg',
    },
    VTextField: {
      variant: 'outlined',
      density: 'comfortable',
    },
    VSelect: {
      variant: 'outlined',
      density: 'comfortable',
    },
    VChip: {
      rounded: 'lg',
    },
    VDataTable: {
      hover: true,
    },
  },
})
