function characterInitials(name: string) {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "tz";
  return parts.slice(0, 2).map((part) => part[0]?.toUpperCase() ?? "").join("");
}

export function CharacterPortraitMark({
  alt = "",
  className = "avatar",
  name,
  src,
}: {
  alt?: string;
  className?: string;
  name: string;
  src: string | null;
}) {
  if (src) {
    return <img alt={alt} className={className} src={src} />;
  }
  return (
    <div aria-hidden="true" className={className}>
      {characterInitials(name)}
    </div>
  );
}
