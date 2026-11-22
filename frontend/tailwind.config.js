/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        rust: {
          50: '#fff5f2',
          100: '#ffe8e1',
          200: '#ffd0c4',
          300: '#ffa794',
          400: '#f87153',
          500: '#e05a47', // Rust orange primary
          600: '#c84735',
          700: '#a83526',
          800: '#8b2e22',
          900: '#732a20',
          950: '#3f120c',
        },
      },
    },
  },
  plugins: [],
};
