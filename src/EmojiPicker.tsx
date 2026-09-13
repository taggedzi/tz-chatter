import { useEffect, useMemo, useRef, useState } from "react";
import { EMOJI_CATEGORIES, filterEmoji, type EmojiCategory } from "./emojiCatalog";

export function EmojiPicker({
  onPick,
}: {
  onPick: (glyph: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<EmojiCategory>("smileys");
  const searchRef = useRef<HTMLInputElement | null>(null);
  const searching = query.trim().length > 0;
  const results = useMemo(
    () => filterEmoji(query, searching ? undefined : category),
    [category, query, searching],
  );

  useEffect(() => {
    searchRef.current?.focus();
  }, []);

  return (
    <div className="emoji-picker" id="emoji-picker" role="dialog" aria-label="Emoji picker">
      <input
        aria-label="Search emoji"
        onChange={(event) => setQuery(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            event.stopPropagation();
          }
        }}
        placeholder="Search emoji"
        ref={searchRef}
        type="search"
        value={query}
      />
      <div className="emoji-picker-categories" role="tablist" aria-label="Emoji categories">
        {EMOJI_CATEGORIES.map((item) => (
          <button
            aria-label={item.label}
            aria-selected={category === item.id}
            className={category === item.id ? "active" : undefined}
            key={item.id}
            onClick={() => setCategory(item.id)}
            role="tab"
            type="button"
          >
            {item.icon}
          </button>
        ))}
      </div>
      <div className="emoji-picker-grid" role="listbox" aria-label={searching ? "Search results" : category}>
        {results.length === 0 ? (
          <p className="emoji-picker-empty">No matches</p>
        ) : (
          results.map((item) => (
            <button
              aria-label={item.name}
              key={`${item.category}:${item.name}`}
              onClick={() => onPick(item.glyph)}
              title={item.name}
              type="button"
            >
              {item.glyph}
            </button>
          ))
        )}
      </div>
    </div>
  );
}
