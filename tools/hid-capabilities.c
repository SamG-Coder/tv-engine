// Read-only HID capability probe. No feature reports or device writes.
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/ioctl.h>
#include <linux/hidraw.h>
int main(void){
 int fd=open("/dev/hidraw0",O_RDONLY|O_NONBLOCK);if(fd<0){perror("open");return 1;}
 int size=0;if(ioctl(fd,HIDIOCGRDESCSIZE,&size)<0){perror("descriptor size");close(fd);return 2;}
 struct hidraw_report_descriptor report={0};if(size<0||size>HID_MAX_DESCRIPTOR_SIZE)return 3;report.size=size;
 if(ioctl(fd,HIDIOCGRDESC,&report)<0){perror("descriptor");close(fd);return 4;}
 printf("descriptor_bytes=%u\n",report.size);for(unsigned i=0;i<report.size;i++)printf("%02x%s",report.value[i],(i+1)%16?" ":"\n");puts("");close(fd);return 0;
}
