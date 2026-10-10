import type { ReactNode } from 'react';

type SectionHeaderProps = {
  title: string;
  description?: string;
  right?: ReactNode;
};

export function SectionHeader({ title, description, right }: SectionHeaderProps) {
  return (
    <div className="section-header">
      <div style={{ minWidth: 0 }}>
        <div className="section-header-title">{title}</div>
        {description ? <div className="section-header-description">{description}</div> : null}
      </div>
      {right}
    </div>
  );
}
