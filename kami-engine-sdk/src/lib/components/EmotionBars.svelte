<script lang="ts">
  import type { EmotionScores, EmotionAxis } from '../types/voice.js';

  interface Props {
    /** Current emotion scores (0–1 per axis). */
    scores: EmotionScores;
    /** Additional CSS class. */
    class?: string;
  }

  let { scores, class: className }: Props = $props();

  const AXES: { key: EmotionAxis; label: string; color: string }[] = [
    { key: 'joy', label: 'Joy', color: 'bg-yellow-400' },
    { key: 'anger', label: 'Anger', color: 'bg-red-500' },
    { key: 'sadness', label: 'Sadness', color: 'bg-blue-400' },
    { key: 'surprise', label: 'Surprise', color: 'bg-orange-400' },
    { key: 'fear', label: 'Fear', color: 'bg-purple-400' },
    { key: 'disgust', label: 'Disgust', color: 'bg-green-500' },
    { key: 'contempt', label: 'Contempt', color: 'bg-gray-400' },
    { key: 'excitement', label: 'Excite', color: 'bg-pink-400' },
  ];
</script>

<div class="flex flex-col gap-1 {className ?? ''}">
  {#each AXES as axis (axis.key)}
    <div class="flex items-center gap-2 text-xs">
      <span class="w-14 text-white/50 truncate">{axis.label}</span>
      <div class="flex-1 h-2 bg-white/10 rounded-full overflow-hidden">
        <div
          class="{axis.color} h-full rounded-full transition-all duration-200"
          style="width: {Math.round(scores[axis.key] * 100)}%"
        ></div>
      </div>
      <span class="w-6 text-right text-white/40">{Math.round(scores[axis.key] * 100)}</span>
    </div>
  {/each}
</div>
