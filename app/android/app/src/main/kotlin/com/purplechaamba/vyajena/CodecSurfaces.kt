package com.purplechaamba.vyajena

import android.graphics.SurfaceTexture
import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLExt
import android.opengl.EGLSurface
import android.opengl.GLES11Ext
import android.opengl.GLES20
import android.opengl.Matrix
import android.view.Surface
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.FloatBuffer
import java.util.concurrent.Semaphore
import java.util.concurrent.TimeUnit

/**
 * video-messages: the GL stage of the Android transcode (design D3, spike S1).
 *
 * MediaCodec's encoder takes its frames from an input [Surface] that must be
 * driven through OpenGL ES — you cannot hand a decoder's output buffers to it
 * directly. So the decoder renders each frame into a [SurfaceTexture]
 * ([OutputSurface]), that texture is drawn onto the encoder's input surface
 * ([InputSurface]) with a trivial shader, and the swap carries the frame's
 * timestamp. Without this stage the encoder is fed nothing and emits only its
 * codec-config, so the muxed MP4 has an audio track but an empty video track —
 * the "plays sound, shows no picture" bug. This is the decode -> GL -> encode
 * blit VideoNormalize's Transcoder plugs into.
 *
 * Adapted from the Android CTS media helpers (ExtractDecodeEditEncodeMuxTest /
 * Grafika), which are the reference for surface-to-surface transcoding.
 */

/** An EGL window surface over the encoder's input [Surface]. Owns the EGL context. */
class InputSurface(private val surface: Surface) {
    private var eglDisplay: EGLDisplay = EGL14.EGL_NO_DISPLAY
    private var eglContext: EGLContext = EGL14.EGL_NO_CONTEXT
    private var eglSurface: EGLSurface = EGL14.EGL_NO_SURFACE

    init {
        eglDisplay = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)
        require(eglDisplay != EGL14.EGL_NO_DISPLAY) { "no EGL display" }
        val version = IntArray(2)
        require(EGL14.eglInitialize(eglDisplay, version, 0, version, 1)) { "eglInitialize failed" }

        // A recordable RGBA config so the surface can feed a MediaCodec encoder.
        val attribs = intArrayOf(
            EGL14.EGL_RED_SIZE, 8,
            EGL14.EGL_GREEN_SIZE, 8,
            EGL14.EGL_BLUE_SIZE, 8,
            EGL14.EGL_ALPHA_SIZE, 8,
            EGL14.EGL_RENDERABLE_TYPE, EGL14.EGL_OPENGL_ES2_BIT,
            EGL_RECORDABLE_ANDROID, 1,
            EGL14.EGL_NONE,
        )
        val configs = arrayOfNulls<EGLConfig>(1)
        val numConfigs = IntArray(1)
        require(
            EGL14.eglChooseConfig(eglDisplay, attribs, 0, configs, 0, 1, numConfigs, 0),
        ) { "eglChooseConfig failed" }

        val contextAttribs = intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, 2, EGL14.EGL_NONE)
        eglContext = EGL14.eglCreateContext(
            eglDisplay, configs[0], EGL14.EGL_NO_CONTEXT, contextAttribs, 0,
        )
        checkEglError("eglCreateContext")

        val surfaceAttribs = intArrayOf(EGL14.EGL_NONE)
        eglSurface = EGL14.eglCreateWindowSurface(
            eglDisplay, configs[0], surface, surfaceAttribs, 0,
        )
        checkEglError("eglCreateWindowSurface")
    }

    fun makeCurrent() {
        require(
            EGL14.eglMakeCurrent(eglDisplay, eglSurface, eglSurface, eglContext),
        ) { "eglMakeCurrent failed" }
    }

    /** The timestamp carried into the muxed video track for the frame just drawn. */
    fun setPresentationTime(nsecs: Long) {
        EGLExt.eglPresentationTimeANDROID(eglDisplay, eglSurface, nsecs)
    }

    fun swapBuffers(): Boolean = EGL14.eglSwapBuffers(eglDisplay, eglSurface)

    fun release() {
        if (eglDisplay != EGL14.EGL_NO_DISPLAY) {
            EGL14.eglMakeCurrent(
                eglDisplay, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_CONTEXT,
            )
            EGL14.eglDestroySurface(eglDisplay, eglSurface)
            EGL14.eglDestroyContext(eglDisplay, eglContext)
            EGL14.eglReleaseThread()
            EGL14.eglTerminate(eglDisplay)
        }
        surface.release()
        eglDisplay = EGL14.EGL_NO_DISPLAY
        eglContext = EGL14.EGL_NO_CONTEXT
        eglSurface = EGL14.EGL_NO_SURFACE
    }

    private fun checkEglError(op: String) {
        val error = EGL14.eglGetError()
        require(error == EGL14.EGL_SUCCESS) { "$op: EGL error 0x${Integer.toHexString(error)}" }
    }

    private companion object {
        const val EGL_RECORDABLE_ANDROID = 0x3142
    }
}

/**
 * A [SurfaceTexture]-backed [Surface] the decoder renders into, plus the shader
 * that draws that texture onto whatever EGL surface is current (the encoder's
 * input, via [InputSurface]). Must be constructed with [InputSurface.makeCurrent]
 * already done so the GL texture lives in the encoder context.
 */
class OutputSurface : SurfaceTexture.OnFrameAvailableListener {
    private val textureRender = TextureRender().apply { setup() }
    private val surfaceTexture = SurfaceTexture(textureRender.textureId)
    val surface: Surface = Surface(surfaceTexture)

    // One permit per rendered frame. A Semaphore (rather than a wait/notify
    // monitor) is robust to the callback arriving on any thread, which is what
    // SurfaceTexture does when the creating thread has no Looper.
    private val frameReady = Semaphore(0)

    init {
        surfaceTexture.setOnFrameAvailableListener(this)
    }

    override fun onFrameAvailable(st: SurfaceTexture) {
        frameReady.release()
    }

    /** Blocks until the decoder has rendered a frame, then latches it into the texture. */
    fun awaitNewImage() {
        check(frameReady.tryAcquire(TIMEOUT_MS, TimeUnit.MILLISECONDS)) {
            "timed out waiting for a decoded frame"
        }
        surfaceTexture.updateTexImage()
    }

    /** Draws the latched frame onto the current EGL surface. */
    fun drawImage() = textureRender.draw(surfaceTexture)

    fun release() {
        surface.release()
        surfaceTexture.release()
    }

    private companion object {
        const val TIMEOUT_MS = 5_000L
    }
}

/** Draws a [SurfaceTexture]'s external texture full-frame with the transform it carries. */
private class TextureRender {
    var textureId = -1
        private set

    private var program = 0
    private var uMvpHandle = 0
    private var uStHandle = 0
    private var aPositionHandle = 0
    private var aTextureHandle = 0

    private val vertices: FloatBuffer = floatBuffer(
        // x, y, z, u, v for a full-screen triangle strip.
        floatArrayOf(
            -1f, -1f, 0f, 0f, 0f,
            1f, -1f, 0f, 1f, 0f,
            -1f, 1f, 0f, 0f, 1f,
            1f, 1f, 0f, 1f, 1f,
        ),
    )
    private val mvp = FloatArray(16).also { Matrix.setIdentityM(it, 0) }
    private val stMatrix = FloatArray(16).also { Matrix.setIdentityM(it, 0) }

    fun setup() {
        program = buildProgram(VERTEX_SHADER, FRAGMENT_SHADER)
        aPositionHandle = GLES20.glGetAttribLocation(program, "aPosition")
        aTextureHandle = GLES20.glGetAttribLocation(program, "aTextureCoord")
        uMvpHandle = GLES20.glGetUniformLocation(program, "uMVPMatrix")
        uStHandle = GLES20.glGetUniformLocation(program, "uSTMatrix")

        val textures = IntArray(1)
        GLES20.glGenTextures(1, textures, 0)
        textureId = textures[0]
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, textureId)
        GLES20.glTexParameteri(
            GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR,
        )
        GLES20.glTexParameteri(
            GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR,
        )
        GLES20.glTexParameteri(
            GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE,
        )
        GLES20.glTexParameteri(
            GLES11Ext.GL_TEXTURE_EXTERNAL_OES, GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE,
        )
    }

    fun draw(st: SurfaceTexture) {
        st.getTransformMatrix(stMatrix)
        GLES20.glClearColor(0f, 0f, 0f, 1f)
        GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT)
        GLES20.glUseProgram(program)

        GLES20.glActiveTexture(GLES20.GL_TEXTURE0)
        GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, textureId)

        vertices.position(0)
        GLES20.glVertexAttribPointer(
            aPositionHandle, 3, GLES20.GL_FLOAT, false, STRIDE, vertices,
        )
        GLES20.glEnableVertexAttribArray(aPositionHandle)

        vertices.position(3)
        GLES20.glVertexAttribPointer(
            aTextureHandle, 2, GLES20.GL_FLOAT, false, STRIDE, vertices,
        )
        GLES20.glEnableVertexAttribArray(aTextureHandle)

        GLES20.glUniformMatrix4fv(uMvpHandle, 1, false, mvp, 0)
        GLES20.glUniformMatrix4fv(uStHandle, 1, false, stMatrix, 0)
        GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4)
        GLES20.glFinish()
    }

    private fun buildProgram(vertexSrc: String, fragmentSrc: String): Int {
        val vs = compile(GLES20.GL_VERTEX_SHADER, vertexSrc)
        val fs = compile(GLES20.GL_FRAGMENT_SHADER, fragmentSrc)
        val prog = GLES20.glCreateProgram()
        GLES20.glAttachShader(prog, vs)
        GLES20.glAttachShader(prog, fs)
        GLES20.glLinkProgram(prog)
        val status = IntArray(1)
        GLES20.glGetProgramiv(prog, GLES20.GL_LINK_STATUS, status, 0)
        require(status[0] == GLES20.GL_TRUE) {
            "program link failed: ${GLES20.glGetProgramInfoLog(prog)}"
        }
        return prog
    }

    private fun compile(type: Int, src: String): Int {
        val shader = GLES20.glCreateShader(type)
        GLES20.glShaderSource(shader, src)
        GLES20.glCompileShader(shader)
        val status = IntArray(1)
        GLES20.glGetShaderiv(shader, GLES20.GL_COMPILE_STATUS, status, 0)
        require(status[0] != 0) { "shader compile failed: ${GLES20.glGetShaderInfoLog(shader)}" }
        return shader
    }

    private fun floatBuffer(data: FloatArray): FloatBuffer =
        ByteBuffer.allocateDirect(data.size * 4).order(ByteOrder.nativeOrder()).asFloatBuffer()
            .apply {
                put(data)
                position(0)
            }

    private companion object {
        const val STRIDE = 5 * 4

        const val VERTEX_SHADER = """
            uniform mat4 uMVPMatrix;
            uniform mat4 uSTMatrix;
            attribute vec4 aPosition;
            attribute vec4 aTextureCoord;
            varying vec2 vTextureCoord;
            void main() {
                gl_Position = uMVPMatrix * aPosition;
                vTextureCoord = (uSTMatrix * aTextureCoord).xy;
            }
        """

        const val FRAGMENT_SHADER = """
            #extension GL_OES_EGL_image_external : require
            precision mediump float;
            varying vec2 vTextureCoord;
            uniform samplerExternalOES sTexture;
            void main() {
                gl_FragColor = texture2D(sTexture, vTextureCoord);
            }
        """
    }
}
