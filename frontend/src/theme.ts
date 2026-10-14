import { useEffect, useState } from 'react';

export type ThemeId = 'cyan' | 'emerald' | 'amber' | 'purple' | 'obsidian';

export interface ThemeConfig {
  id: ThemeId;
  name: string;
  shortName: string;
  primary: string;
  previewColor: string;
  glow: string;
  description: string;
}

export const THEMES: ThemeConfig[] = [
  {
    id: 'cyan',
    name: 'Cyber Cyan',
    shortName: 'Cyan',
    primary: '#00f0ff',
    previewColor: '#00f0ff',
    glow: 'rgba(0, 240, 255, 0.45)',
    description: 'Electric Tron Neon Cyan on Deep Cyber-Obsidian (Default)',
  },
  {
    id: 'emerald',
    name: 'Matrix Emerald',
    shortName: 'Emerald',
    primary: '#00ff88',
    previewColor: '#00ff88',
    glow: 'rgba(0, 255, 136, 0.45)',
    description: 'Classic phosphor terminal green on black void',
  },
  {
    id: 'amber',
    name: 'Solar Amber',
    shortName: 'Amber',
    primary: '#f59e0b',
    previewColor: '#fbbf24',
    glow: 'rgba(245, 158, 11, 0.45)',
    description: 'Retro mainframe CRT amber on dark carbon',
  },
  {
    id: 'purple',
    name: 'Synth Violet',
    shortName: 'Violet',
    primary: '#c084fc',
    previewColor: '#c084fc',
    glow: 'rgba(192, 132, 252, 0.45)',
    description: 'Cyberpunk electric violet & neon magenta',
  },
  {
    id: 'obsidian',
    name: 'Obsidian Stealth',
    shortName: 'Stealth',
    primary: '#e4e4e7',
    previewColor: '#ffffff',
    glow: 'rgba(255, 255, 255, 0.25)',
    description: 'Minimalist high-contrast monochrome graphite',
  },
];

const THEME_STORAGE_KEY = 'wirellm_theme';

export function getStoredTheme(): ThemeId {
  try {
    const saved = localStorage.getItem(THEME_STORAGE_KEY);
    if (saved && THEMES.some((t) => t.id === saved)) {
      return saved as ThemeId;
    }
  } catch (_) {}
  return 'cyan'; // Default to Cyber Cyan
}

export function applyTheme(theme: ThemeId): void {
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
    document.documentElement.setAttribute('data-theme', theme);
  } catch (_) {}
}

export function useTheme() {
  const [theme, setThemeState] = useState<ThemeId>(getStoredTheme);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const setTheme = (newTheme: ThemeId) => {
    setThemeState(newTheme);
    applyTheme(newTheme);
  };

  const currentConfig = THEMES.find((t) => t.id === theme) || THEMES[0];

  return {
    theme,
    setTheme,
    themes: THEMES,
    currentConfig,
  };
}
