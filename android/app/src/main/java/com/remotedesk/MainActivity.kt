package com.remotedesk

import android.app.Activity
import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.media.MediaCodec
import android.media.MediaFormat
import android.os.Bundle
import android.text.Editable
import android.text.InputFilter
import android.text.InputType
import android.text.TextWatcher
import android.view.*
import android.view.inputmethod.InputMethodManager
import android.widget.*
import java.io.*
import java.net.InetSocketAddress
import java.net.Socket
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import kotlin.concurrent.thread
import kotlin.math.hypot

class MainActivity : Activity() {
    private val BG = Color.parseColor("#12141C")
    private val CARD = Color.parseColor("#222530")
    private val ACCENT = Color.parseColor("#4F7CFF")

    private lateinit var root: FrameLayout
    private lateinit var surface: SurfaceView
    private lateinit var login: LinearLayout
    private lateinit var statusTv: TextView
    private lateinit var connectBtn: Button
    private lateinit var toolbar: LinearLayout
    private lateinit var kbInput: EditText

    private val sender = Executors.newSingleThreadExecutor()
    private var sock: Socket? = null
    private var out: DataOutputStream? = null
    @Volatile private var running = false
    private var remoteW = 0
    private var remoteH = 0
    @Volatile private var surfaceReady = false

    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()
    private fun round(color: Int, r: Int = 14) = GradientDrawable().apply { setColor(color); cornerRadius = dp(r).toFloat() }

    override fun onCreate(b: Bundle?) {
        super.onCreate(b)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        root = FrameLayout(this).apply { setBackgroundColor(Color.BLACK) }

        surface = SurfaceView(this)
        surface.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(h: SurfaceHolder) { surfaceReady = true }
            override fun surfaceChanged(h: SurfaceHolder, f: Int, w: Int, hh: Int) {}
            override fun surfaceDestroyed(h: SurfaceHolder) { surfaceReady = false }
        })
        surface.setOnTouchListener { v, e -> touch(v, e); true }
        root.addView(surface, FrameLayout.LayoutParams(-1, -1, Gravity.CENTER))
        root.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> fit() }

        buildToolbar()
        buildLogin()
        setContentView(root)
    }

    // ---------- UI ----------
    private fun buildLogin() {
        val prefs = getSharedPreferences("rd", MODE_PRIVATE)
        fun tv(t: String, sp: Float, c: Int, bold: Boolean = false) = TextView(this).apply {
            text = t; textSize = sp; setTextColor(c); gravity = Gravity.CENTER
            if (bold) setTypeface(null, Typeface.BOLD)
        }
        fun field(hint: String, type: Int) = EditText(this).apply {
            this.hint = hint; inputType = type; setTextColor(Color.WHITE); setHintTextColor(Color.GRAY)
            textSize = 18f; background = round(CARD, 12); setPadding(dp(16), dp(14), dp(16), dp(14)); setSingleLine()
        }
        val ip = field("PC address, e.g. 192.168.1.20", InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI)
            .apply { setText(prefs.getString("ip", "")) }
        val pin = field("6-digit PIN", InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD)
            .apply { filters = arrayOf(InputFilter.LengthFilter(6)) }
        connectBtn = Button(this).apply {
            text = "Connect"; setTextColor(Color.WHITE); textSize = 18f; isAllCaps = false
            background = round(ACCENT, 14); setTypeface(null, Typeface.BOLD)
        }
        statusTv = tv("Open RemoteDesk Host on your PC, then enter its address and PIN.", 14f, Color.GRAY)

        login = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL; setBackgroundColor(BG); gravity = Gravity.CENTER
            setPadding(dp(28), dp(24), dp(28), dp(24))
            addView(tv("🖥️", 56f, Color.WHITE))
            addView(tv("RemoteDesk", 32f, Color.WHITE, true))
            addView(tv("Your Windows PC, in your pocket", 14f, Color.GRAY),
                LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(28) })
            addView(ip, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(12) })
            addView(pin, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(20) })
            addView(connectBtn, LinearLayout.LayoutParams(-1, dp(56)))
            addView(statusTv, LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(20) })
        }
        root.addView(login, FrameLayout.LayoutParams(-1, -1))

        connectBtn.setOnClickListener {
            val host = ip.text.toString().trim(); val p = pin.text.toString().trim()
            if (host.isEmpty() || p.length != 6) { statusTv.text = "Enter the PC address and the 6-digit PIN."; return@setOnClickListener }
            prefs.edit().putString("ip", host).apply()
            connect(host, p)
        }
    }

    private fun buildToolbar() {
        fun pill(t: String, onClick: () -> Unit) = Button(this).apply {
            text = t; setTextColor(Color.WHITE); isAllCaps = false; textSize = 14f
            background = round(Color.parseColor("#AA222530"), 20); minWidth = 0; minimumWidth = dp(48)
            setOnClickListener { onClick() }
        }
        kbInput = EditText(this).apply {
            setText("x"); setSelection(1); alpha = 0f; inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
            addTextChangedListener(object : TextWatcher {
                var busy = false
                override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) {}
                override fun onTextChanged(s: CharSequence?, st: Int, bf: Int, c: Int) {}
                override fun afterTextChanged(s: Editable?) {
                    if (busy || s == null) return
                    val t = s.toString()
                    if (t.isEmpty()) key(0x08) else if (t.length > 1) t.substring(1).forEach { typeChar(it) }
                    busy = true; s.replace(0, s.length, "x"); busy = false
                }
            })
            setOnKeyListener { _, code, ev ->
                if (ev.action == KeyEvent.ACTION_DOWN && code == KeyEvent.KEYCODE_ENTER) { key(0x0D); true } else false
            }
        }
        toolbar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL; visibility = View.GONE
            addView(pill("⌨ Keyboard") {
                kbInput.requestFocus()
                (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).showSoftInput(kbInput, 0)
            })
            addView(pill("Esc") { key(0x1B) })
            addView(pill("Win") { key(0x5B) })
            addView(pill("✕ Disconnect") { disconnect("Disconnected") })
        }
        root.addView(kbInput, FrameLayout.LayoutParams(dp(1), dp(1)))
        root.addView(toolbar, FrameLayout.LayoutParams(-2, -2, Gravity.TOP or Gravity.END).apply { setMargins(dp(8), dp(8), dp(8), 0) })
    }

    private fun fit() {
        if (remoteW == 0 || root.width == 0) return
        val s = minOf(root.width.toFloat() / remoteW, root.height.toFloat() / remoteH)
        val lp = surface.layoutParams as FrameLayout.LayoutParams
        val nw = (remoteW * s).toInt(); val nh = (remoteH * s).toInt()
        if (lp.width != nw || lp.height != nh) { lp.width = nw; lp.height = nh; surface.layoutParams = lp }
    }

    private fun showStream(on: Boolean) {
        login.visibility = if (on) View.GONE else View.VISIBLE
        toolbar.visibility = if (on) View.VISIBLE else View.GONE
        @Suppress("DEPRECATION")
        window.decorView.systemUiVisibility = if (on) (View.SYSTEM_UI_FLAG_FULLSCREEN or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_LAYOUT_STABLE) else 0
    }

    // ---------- input ----------
    private fun send(t: Int, p: ByteArray) {
        sender.execute {
            try { out?.let { it.writeByte(t); it.writeInt(p.size); it.write(p); it.flush() } } catch (_: IOException) {}
        }
    }
    private fun move(x: Float, y: Float) = send(4, ByteBuffer.allocate(8).putFloat(x).putFloat(y).array())
    private fun button(b: Int, down: Boolean) = send(5, byteArrayOf(b.toByte(), if (down) 1 else 0))
    private fun keyEvent(vk: Int, down: Boolean) =
        send(7, byteArrayOf((vk shr 8).toByte(), vk.toByte(), if (down) 1 else 0))
    private fun key(vk: Int) { keyEvent(vk, true); keyEvent(vk, false) }

    private fun typeChar(c: Char) {
        val shift = c.isUpperCase()
        val vk = when {
            c.isLetter() -> c.uppercaseChar().code
            c.isDigit() -> c.code
            c == ' ' -> 0x20; c == '\n' -> 0x0D
            c == '.' -> 0xBE; c == ',' -> 0xBC; c == '-' -> 0xBD; c == '/' -> 0xBF; c == ';' -> 0xBA
            c == '\'' -> 0xDE; c == '=' -> 0xBB; c == '[' -> 0xDB; c == ']' -> 0xDD; c == '\\' -> 0xDC; c == '`' -> 0xC0
            else -> return
        }
        if (shift) keyEvent(0x10, true); key(vk); if (shift) keyEvent(0x10, false)
    }

    private var sx = 0f; private var sy = 0f; private var downT = 0L
    private var pressed = false; private var scrolled = false; private var lastY = 0f; private var acc = 0f

    private fun touch(v: View, e: MotionEvent) {
        if (out == null) return
        val nx = (e.x / v.width).coerceIn(0f, 1f); val ny = (e.y / v.height).coerceIn(0f, 1f)
        val slop = dp(10)
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> { sx = e.x; sy = e.y; downT = System.currentTimeMillis(); pressed = false; scrolled = false; acc = 0f; move(nx, ny) }
            MotionEvent.ACTION_POINTER_DOWN -> { scrolled = true; lastY = (e.getY(0) + e.getY(1)) / 2 }
            MotionEvent.ACTION_MOVE -> {
                if (e.pointerCount >= 2) {
                    val y = (e.getY(0) + e.getY(1)) / 2
                    acc += (y - lastY) / dp(6); lastY = y
                    val n = acc.toInt()
                    if (n != 0) { acc -= n; send(6, ByteBuffer.allocate(2).putShort(n.toShort()).array()) }
                } else if (!scrolled) {
                    if (!pressed && hypot(e.x - sx, e.y - sy) > slop) { pressed = true; button(0, true) }
                    if (pressed) move(nx, ny)
                }
            }
            MotionEvent.ACTION_UP -> if (!scrolled) {
                if (pressed) { move(nx, ny); button(0, false) }
                else {
                    val b = if (System.currentTimeMillis() - downT > 500) 1 else 0 // long press = right click
                    button(b, true); button(b, false)
                }
            }
        }
    }

    // ---------- network ----------
    private fun connect(ip: String, pin: String) {
        connectBtn.isEnabled = false; statusTv.text = "Connecting..."
        running = true
        thread {
            var codec: MediaCodec? = null
            try {
                val s = Socket(); s.connect(InetSocketAddress(ip, 47990), 5000); s.tcpNoDelay = true; sock = s
                val i = DataInputStream(BufferedInputStream(s.getInputStream(), 1 shl 20))
                val o = DataOutputStream(BufferedOutputStream(s.getOutputStream()))
                o.writeByte(1); o.writeInt(pin.length); o.write(pin.toByteArray()); o.flush()
                val info = MediaCodec.BufferInfo()
                while (running) {
                    val t = i.readUnsignedByte(); val len = i.readInt()
                    val p = ByteArray(len); i.readFully(p)
                    when (t) {
                        2 -> {
                            if (p[0].toInt() == 0) { ui { fail("Wrong PIN") }; return@thread }
                            remoteW = ((p[1].toInt() and 255) shl 8) or (p[2].toInt() and 255)
                            remoteH = ((p[3].toInt() and 255) shl 8) or (p[4].toInt() and 255)
                            var wait = 0; while (!surfaceReady && wait++ < 40) Thread.sleep(50)
                            val f = MediaFormat.createVideoFormat("video/avc", remoteW, remoteH)
                            f.setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                            f.setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 4 shl 20)
                            codec = MediaCodec.createDecoderByType("video/avc").also {
                                it.configure(f, surface.holder.surface, null, 0); it.start() }
                            out = o
                            ui { statusTv.text = ""; connectBtn.isEnabled = true; showStream(true); fit() }
                        }
                        3 -> codec?.let { c ->
                            val idx = c.dequeueInputBuffer(10_000)
                            if (idx >= 0) {
                                val buf = c.getInputBuffer(idx)!!; buf.clear(); buf.put(p, 9, len - 9)
                                c.queueInputBuffer(idx, 0, len - 9, ByteBuffer.wrap(p, 0, 8).long * 1000, 0)
                            }
                            var oi = c.dequeueOutputBuffer(info, 0)
                            while (oi >= 0) { c.releaseOutputBuffer(oi, true); oi = c.dequeueOutputBuffer(info, 0) }
                        }
                    }
                }
            } catch (e: Exception) {
                if (running) ui { fail(if (out == null) "Can't reach the PC. Check the address, Wi-Fi and firewall." else "Connection lost") }
            } finally {
                try { codec?.stop(); codec?.release() } catch (_: Exception) {}
            }
        }
    }

    private fun ui(f: () -> Unit) = runOnUiThread(f)

    private fun fail(msg: String) = disconnect(msg)

    private fun disconnect(msg: String) {
        running = false; out = null
        try { sock?.close() } catch (_: Exception) {}
        remoteW = 0; remoteH = 0
        showStream(false); connectBtn.isEnabled = true; statusTv.text = msg
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() { if (running) disconnect("Disconnected") else super.onBackPressed() }
    override fun onDestroy() { running = false; try { sock?.close() } catch (_: Exception) {}; super.onDestroy() }
}
