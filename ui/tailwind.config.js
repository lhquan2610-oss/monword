/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Segoe UI"', '"Inter"', 'system-ui', 'sans-serif'],
        serif: ['"Times New Roman"', 'Times', 'serif'],
      },
    },
  },
  plugins: [],
};
