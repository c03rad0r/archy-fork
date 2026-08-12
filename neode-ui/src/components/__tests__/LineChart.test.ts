import { describe, it, expect, vi, beforeEach } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import LineChart from '../LineChart.vue'

// Mock canvas context
const mockContext = {
  clearRect: vi.fn(),
  beginPath: vi.fn(),
  moveTo: vi.fn(),
  lineTo: vi.fn(),
  stroke: vi.fn(),
  fill: vi.fn(),
  fillRect: vi.fn(),
  fillText: vi.fn(),
  closePath: vi.fn(),
  setLineDash: vi.fn(),
  save: vi.fn(),
  restore: vi.fn(),
  scale: vi.fn(),
  createLinearGradient: vi.fn().mockReturnValue({
    addColorStop: vi.fn(),
  }),
  canvas: { width: 600, height: 200 },
  strokeStyle: '',
  fillStyle: '',
  lineWidth: 0,
  font: '',
  textAlign: '',
  textBaseline: '',
  globalAlpha: 1,
}

beforeEach(() => {
  vi.clearAllMocks()
  HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue(mockContext)
})

describe('LineChart', () => {
  const sampleDatasets = [
    { label: 'CPU', data: [10, 20, 30, 40, 50], color: '#fb923c' },
  ]

  it('renders a canvas element', () => {
    const wrapper = shallowMount(LineChart, {
      props: { datasets: sampleDatasets },
    })
    expect(wrapper.find('canvas').exists()).toBe(true)
  })

  it('accepts datasets prop', () => {
    const wrapper = shallowMount(LineChart, {
      props: { datasets: sampleDatasets },
    })
    expect(wrapper.exists()).toBe(true)
  })

  it('renders with empty datasets', () => {
    const wrapper = shallowMount(LineChart, {
      props: { datasets: [] },
    })
    expect(wrapper.find('canvas').exists()).toBe(true)
  })

  it('renders with multiple datasets', () => {
    const wrapper = shallowMount(LineChart, {
      props: {
        datasets: [
          { label: 'CPU', data: [10, 20, 30], color: '#fb923c' },
          { label: 'Memory', data: [50, 60, 70], color: '#4ade80' },
        ],
      },
    })
    expect(wrapper.exists()).toBe(true)
  })

  it('accepts optional height and width props', () => {
    const wrapper = shallowMount(LineChart, {
      props: {
        datasets: sampleDatasets,
        height: 300,
        width: 600,
      },
    })
    const canvas = wrapper.find('canvas')
    expect(canvas.attributes('width')).toBe('600')
    expect(canvas.attributes('height')).toBe('300')
  })

  it('uses default width of 400 and height of 180', () => {
    const wrapper = shallowMount(LineChart, {
      props: { datasets: sampleDatasets },
    })
    const canvas = wrapper.find('canvas')
    expect(canvas.attributes('width')).toBe('400')
    expect(canvas.attributes('height')).toBe('180')
  })

  it('renders with dataset containing single data point', () => {
    const wrapper = shallowMount(LineChart, {
      props: {
        datasets: [{ label: 'Test', data: [42], color: '#3b82f6' }],
      },
    })
    expect(wrapper.exists()).toBe(true)
  })

  it('accepts yMax and yLabel props', () => {
    const wrapper = shallowMount(LineChart, {
      props: {
        datasets: sampleDatasets,
        yMax: 100,
        yLabel: 'Percent',
      },
    })
    expect(wrapper.exists()).toBe(true)
  })
})
