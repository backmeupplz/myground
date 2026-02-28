import { containerColor, containerIcon, type ContainerStatus } from "../api";

interface Props {
  container: ContainerStatus;
}

export function ContainerStatusCard({ container }: Props) {
  return (
    <div class="flex items-center gap-3 bg-gray-800 rounded-lg px-4 py-3">
      <span class={`text-lg ${containerColor(container)}`}>
        {containerIcon(container)}
      </span>
      <div class="min-w-0">
        <p class="text-sm text-gray-200 truncate">{container.name}</p>
        <p class={`text-xs ${containerColor(container)}`}>
          {container.status}
        </p>
      </div>
    </div>
  );
}
