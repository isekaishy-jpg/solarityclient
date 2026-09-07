"""Compare compiled liquid SPIR-V with liquid_shader_oracle.py's D3D9 frames.

Requires the Python vulkan package and a Vulkan 1.3 graphics device. Uses only
offscreen images: no window, surface, swapchain, or presentation is created.
"""
import argparse
import json
import struct
from pathlib import Path

import vulkan as v


class Renderer:
    """Own the small offscreen graphics context and all subordinate resources."""
    def __init__(self, shader_directory, extent):
        self.cleanup = []
        self.extent = extent
        self.instance = v.vkCreateInstance(v.VkInstanceCreateInfo(
            pApplicationInfo=v.VkApplicationInfo(apiVersion=v.VK_MAKE_VERSION(1, 3, 0))), None)
        self.cleanup.append(lambda: v.vkDestroyInstance(self.instance, None))
        self.physical = v.vkEnumeratePhysicalDevices(self.instance)[0]
        families = v.vkGetPhysicalDeviceQueueFamilyProperties(self.physical)
        self.family = next(i for i, props in enumerate(families) if props.queueFlags & v.VK_QUEUE_GRAPHICS_BIT)
        self.device = v.vkCreateDevice(self.physical, v.VkDeviceCreateInfo(
            pNext=v.VkPhysicalDeviceDynamicRenderingFeatures(dynamicRendering=True),
            pQueueCreateInfos=[v.VkDeviceQueueCreateInfo(queueFamilyIndex=self.family, pQueuePriorities=[1.])]), None)
        self.cleanup.append(lambda: v.vkDestroyDevice(self.device, None))
        self.queue = v.vkGetDeviceQueue(self.device, self.family, 0)
        self.memory_properties = v.vkGetPhysicalDeviceMemoryProperties(self.physical)
        self.pool = self.own(v.vkCreateCommandPool, v.vkDestroyCommandPool,
                            v.VkCommandPoolCreateInfo(queueFamilyIndex=self.family, flags=v.VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT))
        self.command = v.vkAllocateCommandBuffers(self.device, v.VkCommandBufferAllocateInfo(
            commandPool=self.pool, level=v.VK_COMMAND_BUFFER_LEVEL_PRIMARY, commandBufferCount=1))[0]
        self.uniform, self.uniform_memory = self.buffer(512, v.VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT)
        self.vertices, self.vertex_memory = self.buffer(132, v.VK_BUFFER_USAGE_VERTEX_BUFFER_BIT)
        self.upload, self.upload_memory = self.buffer(512, v.VK_BUFFER_USAGE_TRANSFER_SRC_BIT)
        self.readback, self.readback_memory = self.buffer(extent * extent * 4, v.VK_BUFFER_USAGE_TRANSFER_DST_BIT)
        self.target, self.target_view = self.image(extent, v.VK_FORMAT_R8G8B8A8_UNORM,
                                                   v.VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | v.VK_IMAGE_USAGE_TRANSFER_SRC_BIT)
        self.textures = [self.image(4, v.VK_FORMAT_R32G32B32A32_SFLOAT,
                                   v.VK_IMAGE_USAGE_SAMPLED_BIT | v.VK_IMAGE_USAGE_TRANSFER_DST_BIT) for _ in range(2)]
        self.sampler = self.own(v.vkCreateSampler, v.vkDestroySampler, v.VkSamplerCreateInfo(
            magFilter=v.VK_FILTER_NEAREST, minFilter=v.VK_FILTER_NEAREST,
            mipmapMode=v.VK_SAMPLER_MIPMAP_MODE_NEAREST,
            addressModeU=v.VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
            addressModeV=v.VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
            addressModeW=v.VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE, maxLod=0.))
        self.layouts = [self.own(v.vkCreateDescriptorSetLayout, v.vkDestroyDescriptorSetLayout,
            v.VkDescriptorSetLayoutCreateInfo(pBindings=[v.VkDescriptorSetLayoutBinding(
                binding=0, descriptorType=v.VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER, descriptorCount=1,
                stageFlags=v.VK_SHADER_STAGE_VERTEX_BIT | v.VK_SHADER_STAGE_FRAGMENT_BIT)])),
            self.own(v.vkCreateDescriptorSetLayout, v.vkDestroyDescriptorSetLayout,
            v.VkDescriptorSetLayoutCreateInfo(pBindings=[v.VkDescriptorSetLayoutBinding(
                binding=i, descriptorType=v.VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
                descriptorCount=1, stageFlags=v.VK_SHADER_STAGE_FRAGMENT_BIT) for i in range(2)]))]
        descriptor_pool = self.own(v.vkCreateDescriptorPool, v.vkDestroyDescriptorPool,
            v.VkDescriptorPoolCreateInfo(maxSets=2, pPoolSizes=[
                v.VkDescriptorPoolSize(type=v.VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER, descriptorCount=1),
                v.VkDescriptorPoolSize(type=v.VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER, descriptorCount=2)]))
        self.sets = v.vkAllocateDescriptorSets(self.device, v.VkDescriptorSetAllocateInfo(
            descriptorPool=descriptor_pool, pSetLayouts=self.layouts))
        writes = [v.VkWriteDescriptorSet(dstSet=self.sets[0], dstBinding=0, descriptorCount=1,
            descriptorType=v.VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
            pBufferInfo=[v.VkDescriptorBufferInfo(buffer=self.uniform, offset=0, range=512)])]
        writes += [v.VkWriteDescriptorSet(dstSet=self.sets[1], dstBinding=i, descriptorCount=1,
            descriptorType=v.VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,
            pImageInfo=[v.VkDescriptorImageInfo(sampler=self.sampler, imageView=view,
                imageLayout=v.VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL)]) for i, (_, view) in enumerate(self.textures)]
        v.vkUpdateDescriptorSets(self.device, len(writes), writes, 0, None)
        self.pipeline_layout = self.own(v.vkCreatePipelineLayout, v.vkDestroyPipelineLayout,
            v.VkPipelineLayoutCreateInfo(pSetLayouts=self.layouts))
        self.pipelines = [self.pipeline(shader_directory, name) for name in ('water', 'water-no-specular', 'magma')]

    def own(self, create, destroy, info):
        """Register each handle for reverse-order destruction."""
        handle = create(self.device, info, None)
        self.cleanup.append(lambda: destroy(self.device, handle, None))
        return handle

    def memory(self, requirements, flags):
        """Allocate a compatible dedicated memory block."""
        index = next(i for i in range(self.memory_properties.memoryTypeCount)
                     if requirements.memoryTypeBits & (1 << i)
                     and self.memory_properties.memoryTypes[i].propertyFlags & flags == flags)
        return self.own(v.vkAllocateMemory, v.vkFreeMemory,
                        v.VkMemoryAllocateInfo(allocationSize=requirements.size, memoryTypeIndex=index))

    def buffer(self, size, usage):
        """Create a coherent host-visible buffer, destroyed before its allocation."""
        handle = v.vkCreateBuffer(self.device, v.VkBufferCreateInfo(size=size, usage=usage), None)
        memory = self.memory(v.vkGetBufferMemoryRequirements(self.device, handle),
                            v.VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | v.VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)
        self.cleanup.append(lambda: v.vkDestroyBuffer(self.device, handle, None))
        v.vkBindBufferMemory(self.device, handle, memory, 0)
        return handle, memory

    def image(self, extent, format, usage):
        """Allocate one optimal-tiled image and its color view."""
        handle = v.vkCreateImage(self.device, v.VkImageCreateInfo(imageType=v.VK_IMAGE_TYPE_2D,
            format=format, extent=v.VkExtent3D(width=extent, height=extent, depth=1), mipLevels=1,
            arrayLayers=1, samples=v.VK_SAMPLE_COUNT_1_BIT, tiling=v.VK_IMAGE_TILING_OPTIMAL,
            usage=usage, initialLayout=v.VK_IMAGE_LAYOUT_UNDEFINED), None)
        memory = self.memory(v.vkGetImageMemoryRequirements(self.device, handle), v.VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT)
        self.cleanup.append(lambda: v.vkDestroyImage(self.device, handle, None))
        v.vkBindImageMemory(self.device, handle, memory, 0)
        view = self.own(v.vkCreateImageView, v.vkDestroyImageView, v.VkImageViewCreateInfo(
            image=handle, viewType=v.VK_IMAGE_VIEW_TYPE_2D, format=format, subresourceRange=self.color_range()))
        return handle, view

    @staticmethod
    def color_range():
        return v.VkImageSubresourceRange(aspectMask=v.VK_IMAGE_ASPECT_COLOR_BIT, levelCount=1, layerCount=1)

    def pipeline(self, directory, name):
        """Use the production build's modules and the native 44-byte fixture ABI."""
        stages = []
        for suffix, stage in [('vert', v.VK_SHADER_STAGE_VERTEX_BIT), ('frag', v.VK_SHADER_STAGE_FRAGMENT_BIT)]:
            code = (directory / f'liquid-{name}.{suffix}.spv').read_bytes()
            module = self.own(v.vkCreateShaderModule, v.vkDestroyShaderModule,
                             v.VkShaderModuleCreateInfo(codeSize=len(code), pCode=code))
            stages.append(v.VkPipelineShaderStageCreateInfo(stage=stage, module=module, pName='main'))
        attributes = [v.VkVertexInputAttributeDescription(location=i, binding=0, format=format, offset=offset)
            for i, (format, offset) in enumerate([(v.VK_FORMAT_R32G32B32_SFLOAT, 0),
                (v.VK_FORMAT_R32G32B32_SFLOAT, 12), (v.VK_FORMAT_B8G8R8A8_UNORM, 24),
                (v.VK_FORMAT_R32G32_SFLOAT, 28), (v.VK_FORMAT_R32G32_SFLOAT, 36)])]
        info = v.VkGraphicsPipelineCreateInfo(
            pNext=v.VkPipelineRenderingCreateInfo(pColorAttachmentFormats=[v.VK_FORMAT_R8G8B8A8_UNORM]),
            pStages=stages,
            pVertexInputState=v.VkPipelineVertexInputStateCreateInfo(
                pVertexBindingDescriptions=[v.VkVertexInputBindingDescription(binding=0, stride=44, inputRate=v.VK_VERTEX_INPUT_RATE_VERTEX)],
                pVertexAttributeDescriptions=attributes),
            pInputAssemblyState=v.VkPipelineInputAssemblyStateCreateInfo(topology=v.VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST),
            pViewportState=v.VkPipelineViewportStateCreateInfo(pViewports=[v.VkViewport(x=0., y=float(self.extent),
                width=float(self.extent), height=-float(self.extent), minDepth=0., maxDepth=1.)],
                pScissors=[v.VkRect2D(offset=v.VkOffset2D(x=0, y=0), extent=v.VkExtent2D(width=self.extent, height=self.extent))]),
            pRasterizationState=v.VkPipelineRasterizationStateCreateInfo(polygonMode=v.VK_POLYGON_MODE_FILL,
                cullMode=v.VK_CULL_MODE_NONE, frontFace=v.VK_FRONT_FACE_COUNTER_CLOCKWISE, lineWidth=1.),
            pMultisampleState=v.VkPipelineMultisampleStateCreateInfo(rasterizationSamples=v.VK_SAMPLE_COUNT_1_BIT),
            pColorBlendState=v.VkPipelineColorBlendStateCreateInfo(pAttachments=[v.VkPipelineColorBlendAttachmentState(colorWriteMask=15)]),
            layout=self.pipeline_layout)
        handle = v.vkCreateGraphicsPipelines(self.device, v.VK_NULL_HANDLE, 1, [info], None)[0]
        self.cleanup.append(lambda: v.vkDestroyPipeline(self.device, handle, None))
        return handle

    def write(self, memory, data):
        mapped = v.vkMapMemory(self.device, memory, 0, len(data), 0)
        mapped[:] = data
        v.vkUnmapMemory(self.device, memory)

    def barrier(self, image, old, new, source_stage, destination_stage, source_access, destination_access):
        """Make the offscreen image's previous writes visible to its next use."""
        barrier = v.VkImageMemoryBarrier(oldLayout=old, newLayout=new, srcAccessMask=source_access,
            dstAccessMask=destination_access, srcQueueFamilyIndex=v.VK_QUEUE_FAMILY_IGNORED,
            dstQueueFamilyIndex=v.VK_QUEUE_FAMILY_IGNORED, image=image, subresourceRange=self.color_range())
        v.vkCmdPipelineBarrier(self.command, source_stage, destination_stage, 0, 0, None, 0, None, 1, [barrier])

    @staticmethod
    def region(extent, offset=0):
        return v.VkBufferImageCopy(bufferOffset=offset,
            imageSubresource=v.VkImageSubresourceLayers(aspectMask=v.VK_IMAGE_ASPECT_COLOR_BIT, layerCount=1),
            imageExtent=v.VkExtent3D(width=extent, height=extent, depth=1))

    def render(self, kind, uniform, vertices, textures):
        """Submit one triangle and return tightly packed, top-down RGBA bytes."""
        self.write(self.uniform_memory, uniform)
        self.write(self.vertex_memory, vertices)
        self.write(self.upload_memory, textures)
        cmd = self.command
        v.vkResetCommandBuffer(cmd, 0)
        v.vkBeginCommandBuffer(cmd, v.VkCommandBufferBeginInfo(flags=v.VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT))
        for index, (handle, _) in enumerate(self.textures):
            self.barrier(handle, v.VK_IMAGE_LAYOUT_UNDEFINED, v.VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                v.VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, v.VK_PIPELINE_STAGE_TRANSFER_BIT, 0, v.VK_ACCESS_TRANSFER_WRITE_BIT)
            v.vkCmdCopyBufferToImage(cmd, self.upload, handle, v.VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
                                    1, [self.region(4, index * 256)])
            self.barrier(handle, v.VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, v.VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
                v.VK_PIPELINE_STAGE_TRANSFER_BIT, v.VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT,
                v.VK_ACCESS_TRANSFER_WRITE_BIT, v.VK_ACCESS_SHADER_READ_BIT)
        self.barrier(self.target, v.VK_IMAGE_LAYOUT_UNDEFINED, v.VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
            v.VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, v.VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT, 0, v.VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT)
        v.vkCmdBeginRendering(cmd, v.VkRenderingInfo(renderArea=v.VkRect2D(offset=v.VkOffset2D(x=0, y=0),
            extent=v.VkExtent2D(width=self.extent, height=self.extent)), layerCount=1,
            pColorAttachments=[v.VkRenderingAttachmentInfo(imageView=self.target_view,
                imageLayout=v.VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL, loadOp=v.VK_ATTACHMENT_LOAD_OP_CLEAR,
                storeOp=v.VK_ATTACHMENT_STORE_OP_STORE, clearValue=v.VkClearValue(color=v.VkClearColorValue(float32=[0., 0., 0., 0.])))]))
        v.vkCmdBindPipeline(cmd, v.VK_PIPELINE_BIND_POINT_GRAPHICS, self.pipelines[kind])
        v.vkCmdBindDescriptorSets(cmd, v.VK_PIPELINE_BIND_POINT_GRAPHICS, self.pipeline_layout, 0, 2, self.sets, 0, None)
        v.vkCmdBindVertexBuffers(cmd, 0, 1, [self.vertices], [0])
        v.vkCmdDraw(cmd, 3, 1, 0, 0)
        v.vkCmdEndRendering(cmd)
        self.barrier(self.target, v.VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL, v.VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
            v.VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT, v.VK_PIPELINE_STAGE_TRANSFER_BIT,
            v.VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT, v.VK_ACCESS_TRANSFER_READ_BIT)
        v.vkCmdCopyImageToBuffer(cmd, self.target, v.VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, self.readback, 1, [self.region(self.extent)])
        v.vkEndCommandBuffer(cmd)
        v.vkQueueSubmit(self.queue, 1, [v.VkSubmitInfo(pCommandBuffers=[cmd])], v.VK_NULL_HANDLE)
        v.vkQueueWaitIdle(self.queue)
        size = self.extent * self.extent * 4
        mapped = v.vkMapMemory(self.device, self.readback_memory, 0, size, 0)
        result = bytes(mapped)
        v.vkUnmapMemory(self.device, self.readback_memory)
        return result

    def close(self):
        for destroy in reversed(self.cleanup):
            destroy()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('fixture', type=Path)
    parser.add_argument('shader_directory', type=Path)
    args = parser.parse_args()
    data = args.fixture.read_bytes()
    renderer = Renderer(args.shader_directory, struct.unpack_from('<I', data, 12)[0])
    offset, reports = 0, []
    try:
        while offset < len(data):
            kind, points, palette, extent = struct.unpack_from('<4I', data, offset)
            offset += 16
            uniform, vertices, textures = data[offset:offset+512], data[offset+512:offset+644], data[offset+644:offset+1156]
            offset += 1156
            expected = data[offset:offset+extent*extent*4]
            offset += len(expected)
            actual = renderer.render(kind, uniform, vertices, textures)
            errors = [abs(a - b) for a, b in zip(actual, expected)]
            reports.append(dict(kind=kind, points=points, palette=palette, max_error=max(errors),
                                mean_error=sum(errors)/len(errors), channels_over_one=sum(e > 1 for e in errors),
                                first_actual=list(actual[:4]), first_expected=list(expected[:4])))
    finally:
        renderer.close()
    print(json.dumps(reports, indent=2))
    if any(report['channels_over_one'] for report in reports):
        raise SystemExit('Liquid Vulkan/native comparison exceeded one RGBA8 level')


if __name__ == '__main__':
    main()
