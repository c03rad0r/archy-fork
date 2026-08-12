import type { Meta, StoryObj } from '@storybook/vue3'
import { ref, onMounted } from 'vue'
import ContextMenu from '../ContextMenu.vue'
import ContextMenuItem from '../ContextMenuItem.vue'

const meta: Meta<typeof ContextMenu> = {
  title: 'UI/ContextMenu',
  component: ContextMenu,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof ContextMenu>

export const Default: Story = {
  render: () => ({
    components: { ContextMenu, ContextMenuItem },
    setup() {
      const menuRef = ref<InstanceType<typeof ContextMenu> | null>(null)
      onMounted(() => {
        menuRef.value?.open(200, 100)
      })
      return { menuRef }
    },
    template: `
      <ContextMenu ref="menuRef">
        <ContextMenuItem @click="() => {}">Edit</ContextMenuItem>
        <ContextMenuItem @click="() => {}">Copy</ContextMenuItem>
        <ContextMenuItem @click="() => {}">Share</ContextMenuItem>
        <ContextMenuItem destructive @click="() => {}">Delete</ContextMenuItem>
      </ContextMenu>
    `,
  }),
}
