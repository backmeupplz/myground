interface Props {
  /** Service ID — used to load /api/services/{id}/icon.svg */
  id: string;
  class?: string;
}

export function ServiceIcon({ id, class: className = "w-6 h-6" }: Props) {
  return (
    <img
      src={`/api/services/${id}/icon.svg`}
      alt=""
      class={className}
      loading="lazy"
    />
  );
}
