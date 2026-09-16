import React, {useCallback, useEffect, useMemo, useRef, useState} from 'react';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

type Entry = {
  url: string;
  title: string;
  heading: string;
  anchor: string;
  text: string;
};

type Result = Entry & {score: number};

const FIELDS: Array<{key: keyof Entry; boost: number}> = [
  {key: 'title', boost: 4},
  {key: 'heading', boost: 2},
  {key: 'text', boost: 1},
];

function scoreEntry(entry: Entry, tokens: string[]): number {
  let score = 0;
  let matchedAll = true;
  for (const token of tokens) {
    let tokenScore = 0;
    for (const {key, boost} of FIELDS) {
      const idx = entry[key].toLowerCase().indexOf(token);
      if (idx >= 0) {
        // prefix hits at a word boundary rank higher
        const boundary = idx === 0 || /\W/.test((entry[key] as string)[idx - 1] ?? ' ');
        tokenScore = Math.max(tokenScore, boost * (boundary ? 2 : 1));
      }
    }
    if (tokenScore === 0) {
      matchedAll = false;
      break;
    }
    score += tokenScore;
  }
  return matchedAll ? score : 0;
}

function Highlight({text, tokens}: {text: string; tokens: string[]}) {
  if (!text || tokens.length === 0) return <>{text}</>;
  const lower = text.toLowerCase();
  const ranges: Array<[number, number]> = [];
  for (const token of tokens) {
    let from = 0;
    for (;;) {
      const idx = lower.indexOf(token, from);
      if (idx < 0) break;
      ranges.push([idx, idx + token.length]);
      from = idx + token.length;
    }
  }
  if (ranges.length === 0) return <>{text}</>;
  ranges.sort((a, b) => a[0] - b[0]);
  const merged: Array<[number, number]> = [];
  for (const range of ranges) {
    const last = merged[merged.length - 1];
    if (last && range[0] <= last[1]) last[1] = Math.max(last[1], range[1]);
    else merged.push([...range] as [number, number]);
  }
  const out: React.ReactNode[] = [];
  let cursor = 0;
  merged.forEach(([start, end], i) => {
    if (start > cursor) out.push(text.slice(cursor, start));
    out.push(<mark key={i}>{text.slice(start, end)}</mark>);
    cursor = end;
  });
  if (cursor < text.length) out.push(text.slice(cursor));
  return <>{out}</>;
}

export default function SearchPalette(): React.JSX.Element {
  const {
    siteConfig: {baseUrl},
  } = useDocusaurusContext();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [index, setIndex] = useState<Entry[] | null>(null);
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setOpen((value) => !value);
      } else if (event.key === 'Escape') {
        setOpen(false);
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  useEffect(() => {
    if (open && index === null) {
      fetch(`${baseUrl}search-index.json`)
        .then((response) => response.json() as Promise<Entry[]>)
        .then(setIndex)
        .catch(() => setIndex([]));
    }
    if (open) {
      setQuery('');
      setActive(0);
      setTimeout(() => inputRef.current?.focus(), 10);
    }
  }, [open, index, baseUrl]);

  const tokens = useMemo(
    () => query.toLowerCase().split(/\s+/).filter(Boolean),
    [query],
  );

  const results = useMemo<Result[]>(() => {
    if (!index || tokens.length === 0) return [];
    return index
      .map((entry) => ({...entry, score: scoreEntry(entry, tokens)}))
      .filter((entry) => entry.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, 12);
  }, [index, tokens]);

  const go = useCallback(
    (url: string) => {
      setOpen(false);
      window.location.href = url;
    },
    [],
  );

  useEffect(() => {
    const element = listRef.current?.children[active] as HTMLElement | undefined;
    element?.scrollIntoView({block: 'nearest'});
  }, [active]);

  useEffect(() => setActive(0), [query]);

  return (
    <>
      {open && (
        <div className="kpi-overlay" onClick={() => setOpen(false)} role="presentation">
          <div className="kpi-panel" onClick={(event) => event.stopPropagation()} role="dialog" aria-label="Search docs">
            <input
              ref={inputRef}
              className="kpi-input"
              placeholder="Search the ign docs…"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'ArrowDown') {
                  event.preventDefault();
                  setActive((value) => Math.min(value + 1, results.length - 1));
                } else if (event.key === 'ArrowUp') {
                  event.preventDefault();
                  setActive((value) => Math.max(value - 1, 0));
                } else if (event.key === 'Enter' && results[active]) {
                  event.preventDefault();
                  go(results[active].url);
                }
              }}
            />
            <ul className="kpi-results" ref={listRef}>
              {results.map((result, i) => (
                <li
                  key={`${result.url}-${i}`}
                  className={i === active ? 'kpi-hit active' : 'kpi-hit'}
                  onMouseEnter={() => setActive(i)}
                  onClick={() => go(result.url)}>
                  <div className="kpi-hit-title">
                    <Highlight text={result.title} tokens={tokens} />
                    {result.heading && (
                      <span className="kpi-hit-heading">
                        {' '}
                        → <Highlight text={result.heading} tokens={tokens} />
                      </span>
                    )}
                  </div>
                  <div className="kpi-hit-text">
                    <Highlight text={result.text} tokens={tokens} />
                  </div>
                </li>
              ))}
              {index !== null && results.length === 0 && query.trim() !== '' && (
                <li className="kpi-empty">No matches.</li>
              )}
            </ul>
            <div className="kpi-footer">
              <span>↑↓ navigate</span>
              <span>↵ open</span>
              <span>esc close</span>
            </div>
          </div>
        </div>
      )}
      {!open && (
        <button
          type="button"
          className="kpi-pill"
          aria-label="Search docs (Cmd+K)"
          onClick={() => setOpen(true)}>
          <kbd>⌘K</kbd> search
        </button>
      )}
    </>
  );
}
