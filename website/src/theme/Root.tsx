import React from 'react';
import SearchPalette from '@site/src/components/SearchPalette';

// Theme wrapper (documented Docusaurus escape hatch): mounts the cmd+k
// palette alongside the normal page tree without swizzling any core piece.
export default function Root({children}: {children: React.ReactNode}): React.JSX.Element {
  return (
    <>
      <SearchPalette />
      {children}
    </>
  );
}
