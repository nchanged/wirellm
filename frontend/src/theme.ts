import { useEffect, useState } from 'react';

export type ThemeId = 'magenta' | 'cyan' | 'synthwave' | 'emerald' | 'amber' | 'purple' | 'obsidian';

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
    id: 'magenta',
    name: 'Neon Magenta',
    shortName: 'Magenta',
    primary: '#ff1493',
    previewColor: '#ff1493',
    glow: 'rgba(255, 20, 147, 0.5)',
    description: 'Electric Hot Magenta on Cyber-Night Obsidian (Default)',
  },
  {
    id: 'cyan',
    name: 'Cyber Cyan',
    shortName: 'Cyan',
    primary: '#00f0ff',
    previewColor: '#00f0ff',
    glow: 'rgba(0, 240, 255, 0.45)',
    description: 'Electric Tron Neon Cyan on Deep Cyber-Obsidian',
  },
  {
    id: 'synthwave',
    name: 'Synthwave Sunset',
    shortName: 'Synthwave',
    primary: '#f43f5e',
    previewColor: '#f43f5e',
    glow: 'rgba(244, 63, 94, 0.45)',
    description: 'Vibrant hot rose & neon twilight',
  },
  {
    id: 'purple',
    name: 'Acid Violet',
    shortName: 'Violet',
    primary: '#c084fc',
    previewColor: '#c084fc',
    glow: 'rgba(192, 132, 252, 0.45)',
    description: 'Cyberpunk electric violet & neon lavender',
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
  return 'magenta'; // Default to Neon Magenta!
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
