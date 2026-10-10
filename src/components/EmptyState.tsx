type EmptyStateProps = {
  title: string;
  description: string;
};

export function EmptyState({ title, description }: EmptyStateProps) {
  return (
    <div className="empty-state" role="status">
      <div className="empty-state-icon" aria-hidden="true">·</div>
      <div className="empty-state-title">{title}</div>
      <div className="empty-state-subtitle">{description}</div>
    </div>
  );
}

export default EmptyState;
