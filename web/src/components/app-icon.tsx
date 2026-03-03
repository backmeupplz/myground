interface Props {
  /** App ID — used to load /api/apps/{id}/icon.svg */
  id: string;
  class?: string;
}

export function AppIcon({ id, class: className = "w-6 h-6" }: Props) {
  return (
    <img
      src={`/api/apps/${id}/icon.svg`}
      alt=""
      class={className}
      loading="lazy"
    />
  );
}
