import type { ReactNode } from 'react';

type RowProps = {
  children: ReactNode;
  className?: string;
};

export function Row({ children, className }: RowProps) {
  return <div className={`row ${className ?? ''}`}>{children}</div>;
}
