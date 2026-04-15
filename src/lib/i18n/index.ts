import { register, init, locale } from 'svelte-i18n';

export type Lang = 'en' | 'fr' | 'es';

register('en', () => import('./en.json'));
register('fr', () => import('./fr.json'));
register('es', () => import('./es.json'));

export function initI18n(preferred?: Lang) {
  init({
    fallbackLocale: 'en',
    initialLocale: preferred ?? 'en',
  });
}

export function setLang(lang: Lang) {
  locale.set(lang);
}
