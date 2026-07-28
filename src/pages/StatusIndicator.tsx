import React from 'react';

export interface StatusIndicatorProps {
  state: 'active' | 'inactive' | 'pending';
  label?: string;
}

const stateStyles: Record<StatusIndicatorProps['state'], { label: string; border: string; text: string }> = {
  active: { label: 'Active', border: 'var(--status-active-border, #16a34a)', text: 'var(--status-active-text, #14532d)' },
  inactive: { label: 'Inactive', border: 'var(--status-inactive-border, #6b7280)', text: 'var(--status-inactive-text, #374151)' },
  pending: { label: 'Pending', border: 'var(--status-pending-border, #d97706)', text: 'var(--status-pending-text, #92400e)' },
};

export function StatusIndicator({ state, label }: StatusIndicatorProps) {
  const resolvedLabel = label ?? stateStyles[state].label;
  const style = {
    border: `1px solid ${stateStyles[state].border}`,
    color: stateStyles[state].text,
    borderRadius: '9999px',
    padding: '0.25rem 0.75rem',
    display: 'inline-flex',
    alignItems: 'center',
    gap: '0.375rem',
    fontWeight: 600,
  } as React.CSSProperties;

  return (
    <span className="status-indicator" data-state={state} style={style} aria-label={resolvedLabel}>
      {resolvedLabel}
    </span>
  );
}
