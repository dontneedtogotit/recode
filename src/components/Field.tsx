import { cloneElement, isValidElement, useId, type ReactNode } from 'react';

type FieldProps = {
  label: string;
  htmlFor?: string;
  children: ReactNode;
  style?: React.CSSProperties;
};

export function Field({ label, htmlFor, children, style }: FieldProps) {
  const generatedId = useId();
  const control = isValidElement<{ id?: string }>(children)
    && typeof children.type === 'string'
    && ['input', 'select', 'textarea'].includes(children.type) ? children : null;
  const id = htmlFor ?? control?.props.id ?? generatedId;
  return (
    <div className="field" style={style}>
      <label htmlFor={id}>{label}</label>
      {control ? cloneElement(control, { id }) : children}
    </div>
  );
}
