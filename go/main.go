package main

import (
	"fmt"
	"os"
	"os/exec"
	"syscall"

	"golang.org/x/sys/unix"
)

// go run main.go	run			<command>	<args>
// docker			run	<image>	<command>	<args>
func main() {
	if len(os.Args) == 1 {
		fmt.Println("not thing happened")
		return
	}
	switch os.Args[1] {
	case "run":
		run()
	case "child":
		child()
	default:
		fmt.Println("not thing happened")
	}
}

func run() {
	fmt.Println(fmt.Sprintf("Running %v as a user %d in process %d", os.Args[2:], os.Getuid(), os.Getpid()))

	cmd := exec.Command("/proc/self/exe", append([]string{"child"}, os.Args[2:]...)...)
	//cmd := exec.Command(os.Args[2], os.Args[3:]...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	cmd.SysProcAttr = &unix.SysProcAttr{
		Cloneflags: unix.CLONE_NEWUTS | unix.CLONE_NEWUSER | unix.CLONE_NEWNS | unix.CLONE_NEWPID,
		UidMappings: []syscall.SysProcIDMap{
			{
				ContainerID: 0,
				HostID:      1000,
				Size:        1,
			},
		},
		GidMappings: []syscall.SysProcIDMap{
			{
				ContainerID: 0,
				HostID:      1000,
				Size:        1,
			},
		},
	}

	// we can't directly do chroot at this run() function, because it need root permission to execute, but we are still user id 1000 not root(0)
	// the child process will run as the root, and do with root privilege in the container
	// must(syscall.Chroot("/home/peter/filesystem/alpine3.17.1"))
	// must(os.Chdir("/"))
	// must(syscall.Mount("proc", "proc", "proc", 0, ""))
	must(cmd.Run())
	//must(syscall.Unmount("proc", 0))
}

func child() {
	fmt.Println(fmt.Sprintf("Running %v as a user %d in process %d", os.Args[2:], os.Getuid(), os.Getpid()))

	must(syscall.Chroot("/home/peter/filesystem/alpine3.17.1"))
	must(os.Chdir("/"))
	must(syscall.Mount("proc", "proc", "proc", 0, ""))

	cmd := exec.Command(os.Args[2], os.Args[3:]...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	must(cmd.Run())
	must(syscall.Unmount("proc", 0))
}

func must(err error) {
	if err != nil {
		panic(err)
	}
}
