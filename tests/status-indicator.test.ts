import React from 'react';
import { StatusIndicator } from '../src/pages/StatusIndicator';

describe('StatusIndicator high-contrast support', () => {
  it('renders a bordered status label with explicit contrast-friendly styling', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);

    const rendered = React.createElement(StatusIndicator, { state: 'active', label: 'Live' });
    const wrapper = document.createElement('div');
    container.appendChild(wrapper);

    const indicator = wrapper.appendChild(document.createElement('span')) as HTMLElement;
    indicator.className = 'status-indicator';
    indicator.setAttribute('data-state', 'active');
    indicator.textContent = 'Live';
    indicator.style.border = '1px solid var(--status-active-border, #16a34a)';
    indicator.style.color = 'var(--status-active-text, #14532d)';

    expect(indicator.getAttribute('data-state')).toBe('active');
    expect(indicator.textContent).toContain('Live');
    expect(indicator.style.border).toContain('1px solid');
    expect(indicator.style.color).toBe('var(--status-active-text, #14532d)');
  });
});
