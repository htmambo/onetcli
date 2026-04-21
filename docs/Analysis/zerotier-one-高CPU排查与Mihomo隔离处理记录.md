# zerotier-one 高 CPU 排查与 Mihomo 隔离处理记录
> 整理时间：2026-04-22
> 排查日期：2026-04-21 至 2026-04-22
> 适用对象：Linux 桌面环境下的 ZeroTier 客户端性能异常排查

## 背景
用户反馈 `zerotier-one` 长时间占用极高 CPU，在系统监控工具中接近或达到单核 `100%`。

本次处理目标：
1. 判断高 CPU 是真实业务流量导致，还是控制面路径维护异常导致。
2. 在不破坏现有 ZeroTier 基础连通性的前提下，做一轮最小风险缓解。

## 环境概况
| 项目 | 内容 |
|---|---|
| 主机时区 | `Asia/Shanghai` |
| ZeroTier 版本 | `1.16.1` |
| ZeroTier 服务名 | `zerotier-one.service` |
| 本地物理接口 | `wlo1` |
| 本地 Wi-Fi 地址 | `192.168.50.81/24` |
| ZeroTier 接口 | `ztvx34uj3c` |
| ZeroTier 地址 | `10.121.21.111/24` |
| 额外虚拟接口 | `Mihomo` |
| Mihomo 地址 | `198.18.0.1/30`、`fdfe:dcba:9876::1/126` |
| ZeroTier 网络名 | `hopingnetwork` |
| ZeroTier 网络 ID | `2018f78def3daa8d` |

## 初始现象
现场先执行：

```bash
ps -eo pid,ppid,ni,pri,psr,pcpu,pmem,etime,stat,comm,args | rg 'zerotier-one|PID'
systemctl status zerotier-one --no-pager -l
```

得到的关键结果：
- 主进程 PID：`1269`
- 运行时长：约 `2 天 11 小时`
- 当前 CPU：约 `29%` 到 `30%`
- 累计 CPU 时间：约 `17 小时 12 分`
- 内存：约 `7.7M`

初步判断：不是内存异常，更像长期 CPU 忙循环。

## 排查过程
### 1. 确认是不是单线程打满
执行：

```bash
ps -L -p 1269 -o pid,tid,psr,pcpu,stat,comm,wchan:32
cat /proc/1269/status
```

结果：
- 共有 `39` 个线程。
- 只有主线程 `TID 1269` 持续处于 `R` 状态并占用约 `29% CPU`。
- 其他线程基本处于睡眠状态。

结论：
- 不是多线程一起跑满。
- 更像主循环被高频事件持续唤醒。

### 2. 排除“网络规模太大”
执行：

```bash
sudo zerotier-cli info
sudo zerotier-cli listnetworks
sudo zerotier-cli listpeers
```

结果：
- 只有 `1` 个 ZeroTier 网络。
- 只有十几个到二十个左右的 peer。
- 网络状态为 `ONLINE`。

结论：
- 不是大规模网络导致的正常高负载。

### 3. 排除“真实业务流量过大”
执行：

```bash
ip -s link show dev ztvx34uj3c
```

结果：
- `RX` 约 `2.5MB`
- `TX` 约 `0.9MB`

结论：
- 实际隧道流量很小。
- CPU 消耗主要不来自真实业务流量。

### 4. 识别重叠路由
执行：

```bash
ip route show table main
sudo zerotier-cli -j listnetworks
```

关键路由如下：

```text
192.168.50.0/24 dev wlo1 proto kernel scope link src 192.168.50.81
192.168.50.0/24 via 10.121.21.50 dev ztvx34uj3c proto static metric 5000
```

结论：
- 当前物理 Wi-Fi 本身就在 `192.168.50.0/24`。
- ZeroTier 又通过托管路由把同一个网段 `192.168.50.0/24` 指向 `10.121.21.50`。
- 这是明确的地址重叠场景。

### 5. 识别额外干扰接口 Mihomo
执行：

```bash
ss -unap | rg 'zerotier-one|:9993'
ip -brief address
ip rule show
```

结果：
- ZeroTier 同时监听 `wlo1` 和 `Mihomo` 上的 `9993/UDP`。
- `Mihomo` 是额外虚拟接口，带有单独策略路由。

结论：
- 除了重叠路由外，ZeroTier 还在 `Mihomo` 上维护 UDP socket 和路径探测。
- 多接口环境会放大路径维护成本。

### 6. 检查是否启用了 bond
执行：

```bash
sudo zerotier-cli bond list
sudo zerotier-cli -j bond list
```

结果：
- `bond list` 为 `NONE`。
- `bond -j` 里仍能看到很多 peer 保留多条 `active path`。

结论：
- 没有启用显式 bond。
- 但 ZeroTier 仍在进行多路径维护和 NAT 穿透探测。

## 根因判断
综合证据后，本次给出的判断如下：
1. 主因不是业务流量，也不是节点规模。
2. 高 CPU 更像控制面路径维护异常。
3. 重叠路由是根风险。
4. `Mihomo` 是明显的放大器。

更具体地说：
- `192.168.50.0/24` 同时出现在物理 LAN 和 ZeroTier 托管路由中。
- ZeroTier 又在 `Mihomo` 这类虚拟接口上维持 `9993/UDP` 监听与路径探测。
- 主线程因此持续忙于 socket、路径、NAT 和 peer 状态维护。

## 实际处理
### 处理目标
先做一轮最小风险缓解：
- 不改 ZeroTier 网络拓扑。
- 不删除现有托管路由。
- 不影响现有基础网络的节点连接。
- 只让 ZeroTier 忽略 `Mihomo`。

### 处理步骤
先确认本地没有现成 `local.conf`：

```bash
sudo ls -la /var/lib/zerotier-one
sudo cat /var/lib/zerotier-one/local.conf
```

确认结果：`/var/lib/zerotier-one/local.conf` 不存在。

随后创建最小配置 `/var/lib/zerotier-one/local.conf`：

```json
{
  "settings": {
    "interfacePrefixBlacklist": [
      "Mihomo"
    ]
  }
}
```

安装并重启：

```bash
sudo install -o zerotier-one -g zerotier-one -m 644 /tmp/zerotier-local.conf /var/lib/zerotier-one/local.conf
sudo systemctl restart zerotier-one
```

处理时间点：写入配置并重启服务的时间约为 `2026-04-22 01:18 CST`。

## 验证结果
### 1. 配置已正确写入
执行：

```bash
sudo cat /var/lib/zerotier-one/local.conf
```

结果：文件内容与预期一致。

### 2. ZeroTier 服务已成功重启
执行：

```bash
systemctl status zerotier-one --no-pager -l
```

结果：
- 新主进程 PID：`860585`
- 服务状态：`active (running)`
- 启动时间：`Wed 2026-04-22 01:18:16 CST`

### 3. Mihomo 上的 9993 监听已消失
执行：

```bash
ss -unap | rg ':9993'
```

处理前存在：
- `192.168.50.81%wlo1:9993`
- `198.18.0.1%Mihomo:9993`
- 对应 IPv6 监听

处理后只剩：
- `192.168.50.81%wlo1:9993`
- `240e:...%wlo1:9993`

结论：
- `interfacePrefixBlacklist: ["Mihomo"]` 已生效。

### 4. CPU 采样明显回落
执行：

```bash
ps -p 860585 -o pid,pcpu,time,etime,stat,cmd
```

结果：
- 当前 CPU 约 `0.1%`
- 进程状态为 `Ssl`

结论：
- 至少在重启后的现场采样中，CPU 已从持续高占用回落。

### 5. 基础连通性仍在
执行：

```bash
sudo zerotier-cli listpeers
```

结果：
- 仍能看到 `PLANET` 和多个 `LEAF` peer。
- 排除 `Mihomo` 没有让 ZeroTier 基础控制面失效。

## 结论
本次处理已经验证两件事：
1. `Mihomo` 确实是 CPU 问题的放大因素。
2. 把 `Mihomo` 从 ZeroTier 物理接口候选列表中排除后，CPU 现场采样明显下降。

但需要明确：
- 这次处理是缓解，不是根修复。
- 真正的结构性问题仍然是本地物理 LAN 与远端托管路由使用同一个网段 `192.168.50.0/24`。

## 后续建议
1. 长期方案优先处理重叠网段，推荐顺序为：改远端 LAN 网段、拆出第二个 ZeroTier 网络、或在远端网关做 NAT / 子网转换。
2. 当前 `interfacePrefixBlacklist: ["Mihomo"]` 风险低、收益明确，建议保留。
3. 若问题复现，优先按以下顺序排查：`ps -L`、`ss -unap | rg ':9993'`、`ip route show table main`、`sudo zerotier-cli listpeers`。

## 回滚方式
如果需要撤销本次缓解处理，可执行：

```bash
sudo rm -f /var/lib/zerotier-one/local.conf
sudo systemctl restart zerotier-one
```

回滚后建议再次验证：

```bash
ss -unap | rg ':9993'
ps -p $(pgrep -x zerotier-one) -o pid,pcpu,time,etime,stat,cmd
```

## 经验沉淀
- **触发信号**：`zerotier-one` 持续高 CPU，但实际隧道流量很小。
- **高概率根因**：重叠托管路由 + 多虚拟接口路径探测。
- **优先动作**：先看 `9993/UDP` 绑定在哪些接口，再看是否存在重叠网段。
- **低风险缓解**：先用 `interfacePrefixBlacklist` 排除明显的虚拟接口。
- **根修复方向**：消除地址重叠，而不是只盯着 CPU 表象。
