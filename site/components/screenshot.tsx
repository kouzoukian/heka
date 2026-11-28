type ScreenshotProps = {
  name: 'controller' | 'settings';
  alt: string;
};

export function Screenshot({ name, alt }: ScreenshotProps) {
  return (
    <span className="not-prose mx-auto block w-full max-w-[842px]">
      {(['light', 'dark'] as const).map((theme) => (
        <img
          key={theme}
          src={`/screenshots/${name}-${theme}.png`}
          alt={alt}
          width={842}
          height={681}
          loading="lazy"
          className={`h-auto w-full rounded-lg border border-fd-border ${
            theme === 'light' ? 'dark:hidden' : 'hidden dark:block'
          }`}
        />
      ))}
    </span>
  );
}
